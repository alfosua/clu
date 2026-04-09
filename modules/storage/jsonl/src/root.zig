/// storage/jsonl — JSONL file-backed persistent session storage.
/// One JSON object per line, one line per message.
/// Session file path: <storage-dir>/<cwd-hash>/<session-id>.jsonl
pub const JsonlStorage = struct {};
