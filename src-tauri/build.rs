use std::path::Path;

/// Directories the build embeds content from. Both are pinned git submodules
/// (issue #72): the catalog and the rule contracts live in-tree, so a clone
/// without them cannot compile. A missing submodule must fail with the init
/// command, not with an `include_str!` backtrace.
const SUBMODULES: [(&str, &str); 2] = [
    ("agency-agents", "vendor/agency-agents/tools.json"),
    ("agency-rules", "vendor/agency-rules/tools.json"),
];

fn main() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri always has a parent")
        .to_path_buf();

    for (name, marker) in SUBMODULES {
        let path = repo_root.join(marker);
        let present = path.metadata().map(|m| m.len() > 0).unwrap_or(false);
        if !present {
            panic!(
                "\n\
                 vendor/{name} is empty.\n\
                 \n\
                 The app embeds the catalog and rule contracts from pinned git\n\
                 submodules. Initialise them and rebuild:\n\
                 \n\
                 \x20   git submodule update --init --recursive\n\
                 \n\
                 A clone with `git clone --recurse-submodules` needs no extra step.\n\
                 A GitHub source archive (Download ZIP / /archive/...tar.gz) does not\n\
                 contain submodule content at all — clone the repository instead.\n",
            );
        }
        println!("cargo:rerun-if-changed={marker}");
    }

    tauri_build::build()
}
