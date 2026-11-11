//! Typed request/result boundary. The UI schedules execute on its background executor.
use crate::git;
use grove_core::data::ReadEvent;
use grove_core::data::ReadRequest;
use grove_core::data::ReadResult;

pub fn execute(workspace: usize, generation: u64, request: ReadRequest) -> ReadEvent {
    let result = match request {
        ReadRequest::Discover { root } => git::discover(&root).map(ReadResult::Workspace),
        ReadRequest::Worktree { root, path } => git::worktree(&root, &path).map(ReadResult::Worktree),
        ReadRequest::History { root, path, revision } => git::history(&root, &path, revision).map(ReadResult::History),
        ReadRequest::File { root, path, revision, file } => git::file(&root, &path, revision, &file).map(ReadResult::File),
    };
    ReadEvent { workspace, generation, result }
}
