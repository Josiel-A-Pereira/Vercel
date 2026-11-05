use super::super::launch_search::Search;
use super::find_installed_app_in;
use crate::system::app::alias_cache::AliasCache;
use crate::tree::live::live_launch::nonce;

struct Apps {
    root: std::path::PathBuf,
    search: Search,
}

impl Drop for Apps {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn installed(entries: &[(&str, &str)]) -> Apps {
    let root = std::env::temp_dir().join(nonce("installed"));
    let apps = root.join("apps");
    std::fs::create_dir_all(&apps).expect("apps dir");
    for (id, body) in entries {
        std::fs::write(
            apps.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nType=Application\n{body}\n"),
        )
        .expect("entry");
    }
    Apps {
        root,
        search: Search {
            data_dirs: vec![apps],
            path_dirs: Vec::new(),
        },
    }
}

fn lookup(apps: &Apps, identifier: &str) -> Option<(String, Option<String>)> {
    let index = AliasCache::new().index(&apps.search.data_dirs);
    find_installed_app_in(identifier, &apps.search, &index).map(|app| (app.name, app.bundle_id))
}

#[test]
fn a_desktop_id_or_its_name_in_any_case_finds_the_entry() {
    let apps = installed(&[(
        "org.gnome.Calculator",
        "Name=Calculator\nExec=gnome-calculator",
    )]);
    let expected = Some((
        "Calculator".to_string(),
        Some("org.gnome.Calculator".to_string()),
    ));
    assert_eq!(lookup(&apps, "org.gnome.Calculator"), expected);
    assert_eq!(lookup(&apps, "org.gnome.Calculator.desktop"), expected);
    assert_eq!(lookup(&apps, "calculator"), expected);
}

#[test]
fn an_unknown_or_ambiguous_name_finds_nothing() {
    let apps = installed(&[
        ("org.example.One", "Name=Editor\nExec=one"),
        ("org.example.Two", "Name=Editor\nExec=two"),
    ]);
    assert_eq!(lookup(&apps, "Editor"), None, "two entries share the name");
    assert_eq!(lookup(&apps, "Nothing"), None);
    assert_eq!(lookup(&apps, ""), None);
    assert_eq!(lookup(&apps, "/usr/bin/one"), None);
}
