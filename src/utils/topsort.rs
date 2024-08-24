use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::Hash;

#[derive(Debug, Eq, PartialEq)]
pub enum TopologicalSortError {
    CycleDetected,
}

type TopoSortResult<Node> = Result<Vec<Node>, TopologicalSortError>;

/// Given a directed graph represented as a list of edges (source, destination),
/// this function uses Kahn's algorithm to return a topological sort of the graph
/// or detect if there's a cycle.
pub fn topo_sort<Node: Hash + Eq + Copy>(edges: &Vec<(Node, Node)>) -> TopoSortResult<Node> {
    // Step 1: Initialize data structures
    let mut edges_by_source: HashMap<Node, Vec<Node>> = HashMap::new();
    let mut incoming_edges_count: HashMap<Node, usize> = HashMap::new();

    // Step 2: Build the graph and count incoming edges for each node
    for (source, destination) in edges {
        incoming_edges_count.entry(*source).or_insert(0);
        edges_by_source
            .entry(*source)
            .or_default()
            .push(*destination);
        *incoming_edges_count.entry(*destination).or_insert(0) += 1;
    }

    // Step 3: Find all nodes with no incoming edges
    let mut no_incoming_edges_q = VecDeque::new();
    for (node, count) in &incoming_edges_count {
        if *count == 0 {
            no_incoming_edges_q.push_back(*node);
        }
    }

    // Step 4: Process nodes with no incoming edges
    let mut sorted = Vec::new();
    while let Some(node) = no_incoming_edges_q.pop_back() {
        sorted.push(node);
        incoming_edges_count.remove(&node);

        // Step 5: Decrease the incoming edge count for each neighbor
        if let Some(neighbors) = edges_by_source.get(&node) {
            for &neighbor in neighbors {
                if let Some(count) = incoming_edges_count.get_mut(&neighbor) {
                    *count -= 1;
                    if *count == 0 {
                        no_incoming_edges_q.push_front(neighbor);
                    }
                }
            }
        }
    }

    // Step 6: Check if there are any remaining nodes with incoming edges
    if incoming_edges_count.is_empty() {
        Ok(sorted)
    } else {
        Err(TopologicalSortError::CycleDetected)
    }
}

#[cfg(test)]
mod tests {
    use super::topo_sort;
    use crate::utils::topsort::TopologicalSortError;

    fn is_valid_sort<Node: Eq>(sorted: &[Node], graph: &[(Node, Node)]) -> bool {
        for (source, dest) in graph {
            let source_pos = sorted.iter().position(|node| node == source);
            let dest_pos = sorted.iter().position(|node| node == dest);
            match (source_pos, dest_pos) {
                (Some(src), Some(dst)) if src < dst => {}
                _ => {
                    return false;
                }
            };
        }
        true
    }

    #[test]
    fn test_simple_graph() {
        let graph = vec![(1, 2), (1, 3), (2, 3), (3, 4), (4, 5), (5, 6), (6, 7)];
        let sort = topo_sort(&graph);
        assert!(sort.is_ok());
        let sort = sort.unwrap();
        assert!(is_valid_sort(&sort, &graph));
        assert_eq!(sort, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_wikipedia_example() {
        let graph = vec![
            (5, 11),
            (7, 11),
            (7, 8),
            (3, 8),
            (3, 10),
            (11, 2),
            (11, 9),
            (11, 10),
            (8, 9),
        ];
        let sort = topo_sort(&graph);
        assert!(sort.is_ok());
        let sort = sort.unwrap();
        assert!(is_valid_sort(&sort, &graph));
    }

    #[test]
    fn test_cyclic_graph() {
        let graph = vec![(1, 2), (2, 3), (3, 4), (4, 5), (4, 2)];
        let sort = topo_sort(&graph);
        assert!(sort.is_err());
        assert_eq!(sort.err().unwrap(), TopologicalSortError::CycleDetected);
    }
}
