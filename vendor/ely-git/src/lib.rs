//! Ely's version control components for GPUI: changes, commits and their graph, diffs, blame, branches, merges and reviews.
#![deny(missing_docs)]

use ely_core::buttons;
use ely_core::data_display;
use ely_core::forms;
use ely_core::i18n;
use ely_core::lists;
use ely_core::navigation;
use ely_core::primitives;
use ely_core::theme;
use ely_core::typography;
use ely_editor as editor;

mod badges;
mod blame;
mod changes;
mod commits;
mod diff;
mod graph;
mod history;
mod merge;
mod merging;
mod refs;
mod review;
mod rows;
mod tags;
#[cfg(all(test, feature = "test-support"))]
mod tests;
mod viewer;

pub use badges::DiffStat;
pub use badges::GitStatusBadge;
pub use blame::Blame;
pub use blame::BlameView;
pub use blame::GitBlameAnnotation;
pub use changes::ChangeAction;
pub use changes::ChangeSection;
pub use changes::Changed;
pub use changes::ChangesList;
pub use changes::CommitInput;
pub use commits::Commit;
pub use commits::CommitItem;
pub use commits::CommitList;
pub use diff::DiffLine;
pub use diff::LineKind;
pub use diff::Stretch;
pub use diff::diff;
pub use diff::mark_words;
pub use diff::pairs;
pub use diff::stat;
pub use graph::GraphRow;
pub use graph::Half;
pub use graph::Stroke;
pub use graph::lanes;
pub use history::FileHistory;
pub use merge::ConflictResolver;
pub use merge::ThreeWayMerge;
pub use merging::Conflict;
pub use merging::MARKER;
pub use merging::Region;
pub use merging::RegionKind;
pub use merging::Take;
pub use merging::conflicts;
pub use merging::marked;
pub use merging::regions;
pub use merging::resolve;
pub use merging::result;
pub use refs::Branch;
pub use refs::BranchList;
pub use refs::BranchSelector;
pub use review::PullRequest;
pub use review::PullRequestCard;
pub use review::PullState;
pub use review::ReviewComment;
pub use review::ReviewNote;
pub use rows::Place;
pub use tags::GitTag;
pub use tags::Stash;
pub use tags::StashAction;
pub use tags::StashList;
pub use tags::TagList;
pub use viewer::DiffLayout;
pub use viewer::DiffScrollHandle;
pub use viewer::DiffViewer;

/// Core's test setup with this chapter's keys.
#[cfg(all(test, feature = "test-support"))]
fn init_for_tests(cx: &mut gpui::App) {
    ely_core::init_for_tests(cx);
    editor::bind_keys(cx);
}
