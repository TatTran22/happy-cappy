use crate::micro_action::MicroAction;
use crate::pet::manifest::{Animation, PetManifest};
use crate::pet::{BehaviorMode, Personality};

pub fn resolve_animation_chain(
    mode: BehaviorMode,
    personality: Personality,
    expression_index: usize,
    action: Option<MicroAction>,
) -> &'static [&'static str] {
    match mode {
        BehaviorMode::Hidden => &["idle"],
        BehaviorMode::Dragging => &["drag", "idle"],
        BehaviorMode::Hovered => match personality {
            Personality::Calm => &["hover-calm", "hover", "idle"],
            Personality::Cheerful => &["hover-cheerful", "hover", "idle"],
            Personality::Lively => &["hover-lively", "hover", "idle"],
        },
        BehaviorMode::Action => match action {
            Some(MicroAction::Nap) => &["sleepy", "idle"],
            Some(MicroAction::CheerUp) => &["happy", "idle"],
            None => &["idle"],
        },
        BehaviorMode::Walking => &["walk-right", "walk", "idle"],
        BehaviorMode::Default => match expression_index % 5 {
            0 => &["idle"],
            1 => &["blink", "idle"],
            2 => &["happy", "idle"],
            3 => &["curious", "idle"],
            _ => &["sleepy", "idle"],
        },
    }
}

pub fn lookup_with_fallback<'a>(
    manifest: &'a PetManifest,
    chain: &[&'static str],
) -> (&'static str, &'a Animation) {
    for &name in chain {
        if let Some(anim) = manifest.animations.get(name) {
            return (name, anim);
        }
    }
    let idle = manifest
        .animations
        .get("idle")
        .expect("manifest validation guarantees 'idle' exists");
    ("idle", idle)
}

/// Like `lookup_with_fallback`, but accepts runtime `&str` names (e.g. `notify-<kind>`,
/// a CLI-supplied `animation_name`) and returns an owned resolved name. The `&'static`
/// version stays for the enum-driven behavior chains.
pub fn lookup_with_fallback_dynamic<'a>(
    manifest: &'a PetManifest,
    chain: &[&str],
) -> (String, &'a Animation) {
    for &name in chain {
        if let Some(anim) = manifest.animations.get(name) {
            return (name.to_string(), anim);
        }
    }
    let idle = manifest
        .animations
        .get("idle")
        .expect("manifest validation guarantees 'idle' exists");
    ("idle".to_string(), idle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pet::manifest::{Animation, FrameGeometry, PetManifest};
    use std::collections::BTreeMap;

    fn fixture_manifest(animation_names: &[&str]) -> PetManifest {
        let mut animations = BTreeMap::new();
        for name in animation_names {
            animations.insert((*name).to_string(), Animation::from_indices(&[0]));
        }
        PetManifest {
            manifest_version: 1,
            id: "fixture".into(),
            display_name: "Fixture".into(),
            spritesheet_path: "x.png".into(),
            frame: FrameGeometry {
                width: 16,
                height: 16,
                columns: 4,
                rows: 1,
            },
            animations,
        }
    }

    #[test]
    fn chain_for_hovered_uses_personality_variant() {
        let calm = resolve_animation_chain(BehaviorMode::Hovered, Personality::Calm, 0, None);
        assert_eq!(calm, &["hover-calm", "hover", "idle"]);

        let cheerful =
            resolve_animation_chain(BehaviorMode::Hovered, Personality::Cheerful, 0, None);
        assert_eq!(cheerful, &["hover-cheerful", "hover", "idle"]);

        let lively = resolve_animation_chain(BehaviorMode::Hovered, Personality::Lively, 0, None);
        assert_eq!(lively, &["hover-lively", "hover", "idle"]);
    }

    #[test]
    fn chain_for_default_cycles_through_5_expressions() {
        let p = Personality::Cheerful;
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 0, None),
            &["idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 1, None),
            &["blink", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 2, None),
            &["happy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 3, None),
            &["curious", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 4, None),
            &["sleepy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Default, p, 5, None),
            &["idle"]
        );
    }

    #[test]
    fn chain_for_action_uses_micro_action_animation() {
        let p = Personality::Cheerful;
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Action, p, 0, Some(MicroAction::Nap)),
            &["sleepy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Action, p, 0, Some(MicroAction::CheerUp)),
            &["happy", "idle"]
        );
        assert_eq!(
            resolve_animation_chain(BehaviorMode::Action, p, 0, None),
            &["idle"]
        );
    }

    #[test]
    fn chain_for_walking_uses_walk_right_then_walk_then_idle() {
        let chain = resolve_animation_chain(BehaviorMode::Walking, Personality::Cheerful, 0, None);
        assert_eq!(chain, &["walk-right", "walk", "idle"]);
    }

    #[test]
    fn chain_for_dragging_is_drag_then_idle() {
        let chain = resolve_animation_chain(BehaviorMode::Dragging, Personality::Cheerful, 0, None);
        assert_eq!(chain, &["drag", "idle"]);
    }

    #[test]
    fn chain_for_hidden_is_idle_only() {
        let chain = resolve_animation_chain(BehaviorMode::Hidden, Personality::Cheerful, 0, None);
        assert_eq!(chain, &["idle"]);
    }

    #[test]
    fn lookup_falls_back_when_specific_missing() {
        let manifest = fixture_manifest(&["idle"]);
        let (name, _) = lookup_with_fallback(&manifest, &["hover-lively", "hover", "idle"]);
        assert_eq!(name, "idle");
    }

    #[test]
    fn lookup_uses_second_tier_when_specific_missing() {
        let manifest = fixture_manifest(&["idle", "hover"]);
        let (name, _) = lookup_with_fallback(&manifest, &["hover-lively", "hover", "idle"]);
        assert_eq!(name, "hover");
    }

    #[test]
    fn lookup_uses_first_tier_when_present() {
        let manifest = fixture_manifest(&["idle", "hover", "hover-lively"]);
        let (name, _) = lookup_with_fallback(&manifest, &["hover-lively", "hover", "idle"]);
        assert_eq!(name, "hover-lively");
    }

    #[test]
    fn lookup_with_fallback_returns_matched_name_verbatim_even_outside_whitelist() {
        // Use a name that exists in the manifest but never appears in the
        // resolver's static chain tables — proves the function no longer
        // silently rewrites unknown names to "idle".
        let manifest = fixture_manifest(&["idle", "notify-running"]);
        let (name, _) = lookup_with_fallback(&manifest, &["notify-running", "idle"]);
        assert_eq!(name, "notify-running");
    }

    #[test]
    fn dynamic_lookup_returns_first_present_name() {
        let manifest = fixture_manifest(&["idle", "notify-running"]);
        let (name, _) = lookup_with_fallback_dynamic(
            &manifest,
            &["notify-deploy", "notify-running", "notify-message", "idle"],
        );
        assert_eq!(name, "notify-running");
    }

    #[test]
    fn dynamic_lookup_falls_back_to_idle() {
        let manifest = fixture_manifest(&["idle"]);
        let (name, _) =
            lookup_with_fallback_dynamic(&manifest, &["notify-running", "notify-message"]);
        assert_eq!(name, "idle");
    }

    #[test]
    fn dynamic_lookup_honors_runtime_string_first() {
        let manifest = fixture_manifest(&["idle", "notify-custom"]);
        let requested = format!("notify-{}", "custom");
        let (name, _) = lookup_with_fallback_dynamic(&manifest, &[requested.as_str(), "idle"]);
        assert_eq!(name, "notify-custom");
    }
}
