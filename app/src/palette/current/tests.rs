use super::*;
use crate::theme::{self, Dynamic, PaneTheme, ThemeChoice, ThemeGroup};
use std::{fs, path::Path};

struct Desktop(PathBuf);
impl Desktop {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("td-omarchy-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn roots(&self) -> Roots {
        Roots {
            primary: self.0.join("state"),
            legacy: self.0.join("config"),
        }
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn stage(root: &Path, bg: &str, name: &str) {
    fs::create_dir_all(root.join("current/theme")).unwrap();
    fs::write(
        root.join("current/theme/colors.toml"),
        format!("background = \"{bg}\"\nforeground = \"#eeeeee\"\naccent = \"#123456\"\n"),
    )
    .unwrap();
    fs::write(root.join("current/theme.name"), name).unwrap();
}
fn tick(cx: &mut gpui::TestAppContext) {
    cx.executor().advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
}

/// Runs the actual detached watcher, not just the parser: a second theme change
/// must work with the same saved choice and invalidate pane caches each time.
#[gpui::test]
fn omarchy_inherit_keeps_watching_and_survives_directory_replacement(
    cx: &mut gpui::TestAppContext,
) {
    let dir = Desktop::new("watch");
    let roots = dir.roots();
    stage(&roots.primary, "#111111", "first-theme");
    let manual = ThemeChoice {
        dynamic: Dynamic::Wood,
        seed: Some("#ff0000".into()),
        text: Some("#00ff00".into()),
        ..theme::house_outer()
    };
    let mut linked = manual.clone();
    linked.follow_omarchy = Some(true);
    cx.update(|cx| {
        crate::palette::init_shipped(cx);
        theme::init_embedded(cx, manual.clone());
        watch(cx, roots.clone());
        theme::select_outer(cx, linked.clone());
        assert_eq!(theme::theme(cx).bg, theme::parse_hex("#111111").unwrap());
        assert_eq!(
            theme::theme(cx).text,
            theme::parse_hex("#eeeeee").unwrap(),
            "manual wheel colours are parked"
        );
    });
    cx.run_until_parked();
    let before = cx.update(|cx| theme::theme_gen(cx));
    let file = roots.primary.join("current/theme/colors.toml");
    let old_time = fs::metadata(&file).unwrap().modified().unwrap();
    // Omarchy stages elsewhere and renames the complete directory into place.
    stage(&dir.0.join("next"), "#222222", "unused");
    fs::File::options()
        .write(true)
        .open(dir.0.join("next/current/theme/colors.toml"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old_time))
        .unwrap();
    fs::rename(roots.primary.join("current/theme"), dir.0.join("old-theme")).unwrap();
    fs::rename(
        dir.0.join("next/current/theme"),
        roots.primary.join("current/theme"),
    )
    .unwrap();
    fs::write(roots.primary.join("current/theme.name"), "second-theme").unwrap();
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), old_time);
    tick(cx);
    cx.update(|cx| {
        assert_eq!(theme::theme(cx).bg, theme::parse_hex("#222222").unwrap());
        assert!(
            theme::theme_gen(cx) > before,
            "following panes must invalidate their memo"
        );
        assert_eq!(theme::outer_choice(cx).follow_omarchy, Some(true));
    });
    stage(&roots.primary, "#333333", "third-theme");
    tick(cx);
    cx.update(|cx| {
        let outer = theme::outer_choice(cx);
        assert_eq!(theme::theme(cx).bg, theme::parse_hex("#333333").unwrap());
        let mut pane = PaneTheme {
            theme: Some(ThemeGroup::of(&manual)),
            inherit_theme: false,
            ..Default::default()
        };
        let independent = theme::resolve(cx, &pane.effective(&outer)).bg;
        assert_ne!(independent, theme::theme(cx).bg);
        pane.follow_theme();
        assert_eq!(
            theme::resolve(cx, &pane.effective(&outer)).bg,
            theme::theme(cx).bg
        );
        let mut off = outer;
        off.follow_omarchy = Some(false);
        assert_eq!(theme::resolve(cx, &off).bg, theme::resolve(cx, &manual).bg);
        assert_eq!(off.seed, manual.seed);
        let mut inverted = linked.clone();
        inverted.invert = true;
        assert_eq!(
            theme::resolve(cx, &inverted).bg,
            theme::parse_hex("#cccccc").unwrap()
        );
        let restored: ThemeChoice = toml::from_str(&toml::to_string(&linked).unwrap()).unwrap();
        assert!(
            restored.follow_omarchy == Some(true),
            "watching intent survives the state file"
        );
        assert_eq!(theme::resolve(cx, &restored).bg, theme::theme(cx).bg);
    });
}

#[gpui::test]
fn omarchy_watch_recovers_from_absent_and_invalid_input_without_losing_intent(
    cx: &mut gpui::TestAppContext,
) {
    let dir = Desktop::new("recover");
    let roots = dir.roots();
    let mut choice = theme::house_outer();
    choice.follow_omarchy = Some(true);
    cx.update(|cx| {
        crate::palette::init_shipped(cx);
        theme::init_embedded(cx, choice.clone());
        watch(cx, roots.clone());
        assert_eq!(current_status(cx), (false, None));
        assert!(
            current(cx).is_none(),
            "unknown palette is not a black palette"
        );
    });
    cx.run_until_parked();
    stage(&roots.primary, "#555555", "arrived");
    tick(cx);
    cx.update(|cx| assert_eq!(theme::theme(cx).bg, theme::parse_hex("#555555").unwrap()));
    fs::write(
        roots.primary.join("current/theme/colors.toml"),
        "background = 'not-a-colour'\n",
    )
    .unwrap();
    tick(cx);
    cx.update(|cx| {
        assert!(!current_status(cx).0);
        assert_eq!(theme::theme(cx).bg, theme::parse_hex("#555555").unwrap());
        assert_eq!(theme::outer_choice(cx).follow_omarchy, Some(true));
    });
    fs::remove_file(roots.primary.join("current/theme/colors.toml")).unwrap();
    tick(cx);
    stage(&roots.primary, "#777777", "recovered");
    tick(cx);
    cx.update(|cx| {
        assert_eq!(current_status(cx), (true, Some("recovered")));
        assert_eq!(theme::theme(cx).bg, theme::parse_hex("#777777").unwrap());
    });
}

#[test]
fn omarchy_legacy_path_does_not_hide_an_invalid_primary_or_an_invalid_ansi_role() {
    let dir = Desktop::new("paths");
    let roots = dir.roots();
    stage(&roots.legacy, "#112233", "legacy");
    assert!(roots.snapshot().source.is_some());
    fs::create_dir_all(&roots.primary).unwrap();
    assert!(
        roots.snapshot().source.is_none(),
        "primary exists, so a missing file stays unavailable"
    );
    let mut state = Current {
        last_good: None,
        name: None,
        available: false,
    };
    stage(&roots.primary, "#445566", "valid");
    state.accept(&roots.snapshot());
    let file = roots.primary.join("current/theme/colors.toml");
    let source = fs::read_to_string(&file).unwrap();
    fs::write(file, format!("{source}bright_cyan = 'oops'\n")).unwrap();
    state.accept(&roots.snapshot());
    assert!(
        !state.available,
        "a colour beyond the preview chips still must validate"
    );
    assert_eq!(
        state.last_good.unwrap().bg,
        theme::parse_hex("#445566").unwrap()
    );
    let old: ThemeChoice = toml::from_str("id = 'custom'").unwrap();
    assert!(
        old.follow_omarchy.is_none(),
        "old state keeps its manual choice"
    );
    assert!(!toml::to_string(&old).unwrap().contains("follow_omarchy"));
    let mut declared = old;
    declared.follow_omarchy = Some(false);
    let back: ThemeChoice = toml::from_str(&toml::to_string(&declared).unwrap()).unwrap();
    assert_eq!(
        back.follow_omarchy,
        Some(false),
        "explicit off survives separately from undeclared"
    );
    let mut group = ThemeGroup::of(&theme::house_outer());
    group.follow_omarchy = Some(true);
    crate::paint::stamp(&mut group, &crate::paint::Pick::Set(Dynamic::Wood));
    assert!(
        group.follow_omarchy == Some(false),
        "a manual paint pick releases the live source"
    );
}
