//! A file under the loose AddOns root, read as the reference's Windows filesystem opens it:
//! sandboxed to the root and matched whatever its case, so BlizzMove's toc entry `Blizzmove.lua`
//! opens `BlizzMove.lua`.

use std::path::{Path, PathBuf};

use super::{collation_key, upcase_unit};

/// `root/rel`, refusing to escape `root`. `rel` is already resolved by
/// [`wrathplus_ui::loader::join_ref`], so an escape shows as a leading `..`, refused with anything
/// absolute before any filesystem call; a symlinked addon folder still works. A miss retries
/// case-folded ([`fold_case_under`]).
pub(super) fn read_under(root: &Path, rel: &str) -> Option<Vec<u8>> {
    let rel = Path::new(rel);
    if rel
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    std::fs::read(root.join(rel))
        .ok()
        .or_else(|| std::fs::read(fold_case_under(root, rel)?).ok())
}

/// `root/rel` with each component matched as NTFS matches a name, through `$UpCase`
/// ([`upcase_unit`]): the exact name first, else the first case variant in directory order.
fn fold_case_under(root: &Path, rel: &Path) -> Option<PathBuf> {
    let fold = |name: &str| name.chars().map(upcase_unit).collect::<String>();
    rel.components().try_fold(root.to_path_buf(), |dir, part| {
        let exact = dir.join(part);
        if exact.exists() {
            return Some(exact);
        }
        let want = fold(part.as_os_str().to_str()?);
        std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| fold(name) == want)
            .min_by_key(|name| collation_key(name))
            .map(|name| dir.join(name))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Addon, Source};
    use super::*;
    use wrathplus_ui::script::UiScript;
    use wrathplus_ui::status::Status;
    use wrathplus_ui::toc::Toc;

    /// A manifest entry, an `<Include>` and a `<Script file=>` open whatever their case, as on the
    /// reference's Windows filesystem.
    #[test]
    fn addon_files_open_whatever_the_case_of_the_reference() {
        let tmp =
            std::env::temp_dir().join(format!("benilla-addon-file-case-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("AddOns");
        std::fs::create_dir_all(root.join("Probe/sub")).unwrap();
        std::fs::write(tmp.join("secret.txt"), "no").unwrap();
        std::fs::write(root.join("Probe/foo.lua"), "FOO = 1").unwrap();
        std::fs::write(
            root.join("Probe/sub/bar.xml"),
            r#"<Ui><Script file="BAZ.lua"/></Ui>"#,
        )
        .unwrap();
        std::fs::write(root.join("Probe/sub/baz.lua"), "BAZ = 1").unwrap();
        let toc = "## Interface: 30300\nFoo.lua\nSub\\Bar.xml\n";
        let addon = Addon {
            name: "Probe".into(),
            toc: Toc::parse(toc),
            source: Source::Dir(root.clone()),
        };

        let script = UiScript::new().unwrap();
        let failures = addon.load_wrath(&script, &mut Status::default());
        assert!(failures.is_empty(), "{failures:?}");
        assert!(script.eval::<bool>("return FOO == 1 and BAZ == 1").unwrap());
        // The exact spelling still wins, and the fold never walks out of the AddOns root.
        assert_eq!(
            read_under(&root, "Probe/foo.lua").as_deref(),
            Some(&b"FOO = 1"[..])
        );
        assert!(read_under(&root, "../SECRET.TXT").is_none());
        assert!(read_under(&root, "PROBE/../../Secret.txt").is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
