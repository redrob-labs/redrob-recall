// SPDX-License-Identifier: Apache-2.0
//! Windows only: the application icon and version resource (resources/icons/redrob-recall.ico, from
//! scripts/build-brand-assets.py). Nothing on the other platforms.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../resources/icons/redrob-recall.ico");
        res.set("ProductName", "Redrob Recall");
        res.set("FileDescription", "Redrob Recall");
        res.set(
            "LegalCopyright",
            "Copyright 2026 Janghoon Lee (Redrob) and contributors",
        );
        if let Err(error) = res.compile() {
            println!("cargo:warning=windows resource not embedded: {error}");
        }
    }
}
