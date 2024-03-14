use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use clap::Parser;
use handlegraph::handle::{Handle, NodeId, Edge};
use handlegraph::handlegraph::*;
use handlegraph::mutablehandlegraph::*;
use handlegraph::pathhandlegraph::*;
use handlegraph::hashgraph::HashGraph;
use gfa::{gfa::GFA, parser::GFAParser};

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// List of GFA file paths.
    #[clap(short, long, value_parser)]
    gfa_list: Vec<String>,
}

fn main() {
    let args = Args::parse();
    let smoothed_block_graphs = args
        .gfa_list
        .iter()
        .map(|gfa_path| {
            let parser = GFAParser::new();
            let gfa: GFA<usize, ()> = parser.parse_file(gfa_path).unwrap();
            HashGraph::from_gfa(&gfa)
        })
        .collect::<Vec<_>>();

    let path_ranges = args
        .gfa_list
        .iter()
        .flat_map(|gfa_path| {
            let parser = GFAParser::new();
            let gfa: GFA<usize, ()> = parser.parse_file(gfa_path).unwrap();
            let graph = HashGraph::from_gfa(&gfa);

            graph.path_ids()
                .filter_map(|path_id| {
                    let path_name = graph.get_path_name(path_id)?.to_vec();
                    let path_name_str = std::str::from_utf8(&path_name).ok()?;
                    if let Some((genome, hap, chr, start, end)) = parse_path_name(path_name_str) {
                        let steps: Vec<PathStep> = graph
                            .get_path_ref(path_id)?
                            .iter()
                            .map(|step| PathStep {
                                block_id: smoothed_block_graphs.len() - 1,
                                node_id: step.handle().id(),
                            })
                            .collect();

                        Some(PathRange {
                            path_name: path_name_str.to_string(),
                            steps,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect::<Vec<_>>();

    let smoothed_graph = lace_smoothed_blocks(&smoothed_block_graphs, &path_ranges);
    // ... (further processing or output of the smoothed graph)
}

pub struct PathRange {
    pub path_name: String,
    pub steps: Vec<PathStep>,
}

pub struct PathStep {
    pub block_id: usize,
    pub node_id: NodeId,
}

pub fn lace_smoothed_blocks(
    smoothed_block_graphs: &[HashGraph],
    path_ranges: &[PathRange],
) -> HashGraph {
    let mut smoothed_graph = HashGraph::new();
    let mut block_id_to_smoothed_id: Vec<HashMap<NodeId, NodeId>> = vec![HashMap::new(); smoothed_block_graphs.len()];

    // Copy nodes and edges from the smoothed blocks to the smoothed graph
    for (block_id, smoothed_block) in smoothed_block_graphs.iter().enumerate() {
        for handle in smoothed_block.handles() {
            let new_node_id = smoothed_graph.create_handle(smoothed_block.sequence(&handle).to_vec(), handle.id().into());
            block_id_to_smoothed_id[block_id].insert(handle.id(), new_node_id.into());
        }

        for edge in smoothed_block.edges() {
            smoothed_graph.create_edge(edge);
        }
    }

    // Create paths in the smoothed graph based on the path ranges
    for path_range in path_ranges {
        let path_id = smoothed_graph.create_path(path_range.path_name.as_bytes(), false).unwrap();

        for step in &path_range.steps {
            let smoothed_node_id = block_id_to_smoothed_id[step.block_id][&step.node_id];
            smoothed_graph.path_append_step(path_id, Handle::pack(smoothed_node_id, false));
        }
    }

    // Connect the path steps in the smoothed graph
    smoothed_graph.with_all_paths_mut_ctx(|_, _, path_ref| {
        let mut prev_step = None;
        for step in path_ref.iter() {
            if let Some(prev) = prev_step {
                smoothed_graph.create_edge(Edge(prev, step));
            }
            prev_step = Some(step);
        }
    });

    smoothed_graph
}

fn parse_path_name(path_name: &str) -> Option<(String, usize, String, usize, usize)> {
    let parts: Vec<&str> = path_name.split('#').collect();
    if parts.len() == 3 {
        let genome = parts[0].to_string();
        let hap = parts[1].parse().ok()?;
        let chr_range: Vec<&str> = parts[2].split(':').collect();
        if chr_range.len() == 2 {
            let chr = chr_range[0].to_string();
            let range: Vec<&str> = chr_range[1].split('-').collect();
            if range.len() == 2 {
                let start = range[0].parse().ok()?;
                let end = range[1].parse().ok()?;
                return Some((genome, hap, chr, start, end));
            }
        }
    }
    None
}