//! Commit-graph projection from the loaded topological parent list, without inventing edges.
use crate::data::CommitSummary;

pub struct GraphRow {
    pub lane: usize,
    pub incoming: bool,
    pub pass_list: Vec<(usize, usize)>,
    pub parent_lane_list: Vec<usize>,
    pub lane_count: usize,
}

pub fn project(commit_list: &[CommitSummary]) -> Vec<GraphRow> {
    let mut lane_list: Vec<String> = Vec::new();
    let mut row_list = Vec::with_capacity(commit_list.len());
    for commit in commit_list {
        let incoming = lane_list.iter().any(|oid| oid == &commit.oid);
        let lane = lane_list.iter().position(|oid| oid == &commit.oid).unwrap_or_else(|| {
            lane_list.push(commit.oid.clone());
            lane_list.len() - 1
        });
        let before_list = lane_list.clone();
        lane_list.remove(lane);
        for (index, parent) in commit.parent_list.iter().enumerate() {
            if !lane_list.contains(parent) {
                lane_list.insert((lane + index).min(lane_list.len()), parent.clone());
            }
        }
        let pass_list = before_list
            .iter()
            .enumerate()
            .filter(|(_, oid)| *oid != &commit.oid)
            .filter_map(|(index, oid)| lane_list.iter().position(|next| next == oid).map(|next| (index, next)))
            .collect();
        let parent_lane_list = commit.parent_list.iter().filter_map(|parent| lane_list.iter().position(|oid| oid == parent)).collect();
        row_list.push(GraphRow { lane, incoming, pass_list, parent_lane_list, lane_count: before_list.len().max(lane_list.len()) });
    }
    row_list
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merge_lanes_join_existing_parent_without_inventing_root_edge() {
        let commit_list: Vec<_> = [("a", vec!["b", "c"]), ("b", vec!["d"]), ("c", vec!["d"]), ("d", vec![])]
            .into_iter()
            .map(|(oid, parent_list)| CommitSummary {
                oid: oid.into(),
                parent_list: parent_list.into_iter().map(str::to_owned).collect(),
                subject: String::new(),
                author: String::new(),
                date: String::new(),
            })
            .collect();
        let graph_list = project(&commit_list);
        assert_eq!(graph_list[0].parent_lane_list, [0, 1]);
        assert!(!graph_list[0].incoming);
        assert_eq!(graph_list[1].pass_list, [(1, 1)]);
        assert_eq!(graph_list[2].lane, 1);
        assert_eq!(graph_list[2].parent_lane_list, [0]);
        assert!(graph_list[3].parent_lane_list.is_empty());
        assert!(graph_list[3].incoming);
    }
}
