# Omni fixtures

Synthetic responses shaped like Omni's documented API (ADR-008), for a made-up coffee roaster called Lumen Coffee. The data is synthetic: every row, name, id and host was written by hand for these tests, and none came from a real Omni instance or a real company.

`recorded.json` lists each recorded request (method, path after `/api`, and the JSON body a `POST` must carry) with the file holding its response. The replay server in `crates/tiles-omni/src/replay.rs` serves these and answers any other request with `501`.
