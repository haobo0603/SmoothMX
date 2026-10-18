# Development setup

Required: Git, stable Rust/Cargo via rustup, rustfmt, Clippy, macOS SDK and Swift. Full Xcode is needed for the planned app signing and distribution workflow. VS Code and GitHub Desktop are optional conveniences.

After installing Xcode, open it once and finish its component/license prompts. If needed, select it with:

```sh
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
xcodebuild -version
rustc --version
cargo --version
```

Build commands are in README.md. Use scripts/build-macos.sh for the linked Rust/Swift app preview. Run Rust checks from the repository root; Swift commands from apps/macos. No external Rust package dependencies are present in the scaffold.

Before release: choose a license, create the app/helper bundle and entitlements, establish Developer ID signing/notarization, verify permission identity across updates, and test install/uninstall without deleting user settings unexpectedly.
