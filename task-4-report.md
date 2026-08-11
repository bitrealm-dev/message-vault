Task 4 report

- Fixed upload progress in `src-tauri/src/commands/push.rs` so `FileStart` is the only source of `extract:progress` updates for upload.
- Added `extract:issue` emission for non-`ok` `FileDone` statuses, tagged as `skip` for skipped files and `error` otherwise.
- Verification: reran the `extract_progress_parser_tracks_parse_and_convert` unit test.
