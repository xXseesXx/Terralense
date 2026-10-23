//! Slim icon-only right rail for per-type structure and ore visibility.
use crate::{
    dimension::Dimension, display::DisplayPrefs, model::MarkerKind, ore_icons, structure_icons,
};
use eframe::egui::{self, Align, Context, Layout, Sense, Vec2};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug)]
struct RailEntry {
    key: String,
    label: String,
    icon_id: String,
    members: Vec<String>,
}

fn structure_rows(entries: &[Entry]) -> Vec<RailEntry> {
    let mut groups = std::collections::BTreeMap::<String, RailEntry>::new();
    for entry in entries {
        let (namespace, path) = entry.id.split_once(':').unwrap_or(("", entry.id.as_str()));
        let (parent, leaf) = path.rsplit_once('/').unwrap_or(("", path));
        let lower = leaf.to_ascii_lowercase();
        let tokens: Vec<_> = lower.split('_').collect();
        let has = |word: &str| tokens.contains(&word);
        // Identify the object before its condition: a ruined castle is still a castle,
        // and a ruined village remains in the village family, rather than generic ruins.
        let family = if lower == "unfinished_sloop" || lower.starts_with("unfinished_sloop_") {
            Some(("unfinished_sloop", "Unfinished sloops"))
        } else if lower == "unfinished_rowboat" || lower.starts_with("unfinished_rowboat_") {
            Some(("unfinished_rowboat", "Unfinished rowboats"))
        } else if has("village") || has("villages") {
            Some(("village", "Villages"))
        } else if has("digsite") || lower.contains("dig_site") {
            Some(("digsite", "Dig sites"))
        } else if has("church") || has("churches") {
            Some(("church", "Churches"))
        } else if has("towerhouse") || has("towerhouses") {
            Some(("towerhouse", "Towerhouses"))
        } else if has("tower") || has("towers") {
            Some(("tower", "Towers"))
        } else if has("castle") || has("castles") {
            Some(("castle", "Castles"))
        } else if has("temple") || has("temples") {
            Some(("temple", "Temples"))
        } else if has("monument") || has("monuments") {
            Some(("monument", "Monuments"))
        } else if has("ruin") || has("ruins") {
            Some(("ruins", "Ruins"))
        } else if has("shipwreck") || has("shipwrecks") {
            Some(("shipwreck", "Shipwrecks"))
        } else if has("stronghold") || has("strongholds") {
            Some(("stronghold", "Strongholds"))
        } else if has("camp") || has("camps") {
            Some(("camp", "Camps"))
        } else {
            None
        };
        let base = lower
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end_matches('_');
        let base = ["_small", "_large"]
            .iter()
            .find_map(|suffix| base.strip_suffix(suffix))
            .unwrap_or(base);
        let (family, label) = family
            .map(|(key, label)| (key, label.to_owned()))
            .unwrap_or_else(|| {
                (
                    base,
                    if base == lower {
                        entry.label.clone()
                    } else {
                        base.replace('_', " ")
                    },
                )
            });
        let key = format!("structure:{namespace}:{parent}/{family}");
        let group = groups.entry(key.clone()).or_insert_with(|| RailEntry {
            key,
            label,
            icon_id: entry.id.clone(),
            members: Vec::new(),
        });
        group.members.push(entry.id.clone());
    }
    finish_rows(groups)
}

/// Mineral families follow the configured primary output, including raw blocks and
/// host-rock variants. Vein IDs remain intact as the individually selectable children.
fn ore_rows(entries: &[Entry]) -> Vec<RailEntry> {
    static FAMILIES: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();
    let families = FAMILIES.get_or_init(|| {
        let mut families = std::collections::HashMap::new();
        for json in [
            include_str!("../../assets/data/tfg/veins.json"),
            include_str!("../beneath/ore-features.json"),
        ] {
            let data: serde_json::Value =
                serde_json::from_str(json).expect("embedded vein catalog");
            for (id, vein) in data["veins"].as_object().into_iter().flatten() {
                let mut weights = std::collections::BTreeMap::<String, u64>::new();
                for (host, outputs) in vein["blocks"].as_object().into_iter().flatten() {
                    let host_leaf = host.rsplit([':', '/']).next().unwrap_or(host);
                    let host_prefix = if host.contains(":sand/") {
                        format!("{host_leaf}_sand_")
                    } else {
                        format!("{host_leaf}_")
                    };
                    for output in outputs.as_array().into_iter().flatten() {
                        let Some(block) = output["block"].as_str() else {
                            continue;
                        };
                        let local = block.rsplit(':').next().unwrap_or(block);
                        let material = local
                            .strip_prefix("raw_")
                            .and_then(|value| value.strip_suffix("_block"))
                            .or_else(|| {
                                local
                                    .strip_prefix(&host_prefix)
                                    .and_then(|value| value.strip_suffix("_ore"))
                            });
                        if let Some(material) = material {
                            *weights.entry(material.to_owned()).or_default() +=
                                output["weight"].as_u64().unwrap_or(1);
                        }
                    }
                }
                if let Some((material, _)) = weights
                    .into_iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                {
                    families.insert(id.clone(), mineral_family(&material).to_owned());
                }
            }
        }
        families
    });
    let mut groups = std::collections::BTreeMap::<String, RailEntry>::new();
    for entry in entries {
        let (namespace, path) = entry.id.split_once(':').unwrap_or(("", entry.id.as_str()));
        let (parent, leaf) = path.rsplit_once('/').unwrap_or(("", path));
        let dimension = if leaf.starts_with("beneath_") {
            "beneath"
        } else {
            "overworld"
        };
        let family = if leaf.ends_with("_dike") {
            "rock_dikes"
        } else {
            families.get(&entry.id).map(String::as_str).unwrap_or(leaf)
        };
        let key = format!("ore:{namespace}:{parent}/{dimension}/{family}");
        let group = groups.entry(key.clone()).or_insert_with(|| RailEntry {
            key,
            label: family.replace('_', " "),
            icon_id: entry.id.clone(),
            members: Vec::new(),
        });
        // Prefer a normal-depth raw ore sprite over a surface or exotic variant.
        if ore_icon_order(&entry.id) < ore_icon_order(&group.icon_id) {
            group.icon_id = entry.id.clone();
        }
        group.members.push(entry.id.clone());
    }
    let mut rows = finish_rows(groups);
    for row in &mut rows {
        if row.members.len() == 1 {
            if let Some(entry) = entries.iter().find(|entry| entry.id == row.icon_id) {
                row.label = entry.label.clone();
            }
        }
    }
    rows
}

fn mineral_family(material: &str) -> &str {
    match material {
        "copper" | "native_copper" | "chalcopyrite" | "chalcocite" | "bornite" | "malachite"
        | "tetrahedrite" => "copper",
        "iron" | "goethite" | "yellow_limonite" | "limonite" | "hematite" | "magnetite"
        | "vanadium_magnetite" => "iron",
        "coal" | "lignite" | "anthracite" => "coal",
        "cassiterite" | "cassiterite_sand" | "tin" => "tin",
        "galena" | "lead" => "lead_and_silver",
        "sphalerite" | "zinc" => "zinc",
        "garnierite" | "pentlandite" | "nickel" => "nickel",
        "quartz" | "quartzite" | "nether_quartz" => "quartz",
        "salt" | "rock_salt" | "sylvite" => "salt",
        "mica" | "phlogopite" => "mica",
        "apatite" | "tricalcium_phosphate" => "phosphate",
        _ => material,
    }
}

fn ore_icon_order(id: &str) -> (u8, &str) {
    (
        if id.starts_with("normal_") {
            0
        } else if id.starts_with("beneath_") {
            1
        } else if id.starts_with("surface_") {
            2
        } else {
            3
        },
        id,
    )
}

fn finish_rows(groups: std::collections::BTreeMap<String, RailEntry>) -> Vec<RailEntry> {
    groups
        .into_values()
        .map(|mut row| {
            row.members.sort();
            row.members.dedup();
            row
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mode {
    #[default]
    Structures,
    Ores,
}

#[derive(Default)]
pub struct State {
    group: Mode,
    expanded: std::collections::HashSet<String>,
}

impl State {
    pub(super) fn mode(&self) -> Mode {
        self.group
    }
    pub(super) fn set_mode(&mut self, mode: Mode) {
        self.group = mode;
    }
    pub(super) fn ores_mode(&self) -> bool {
        self.mode() == Mode::Ores
    }
    pub(super) fn set_ores_mode(&mut self, ores: bool) {
        self.set_mode(if ores { Mode::Ores } else { Mode::Structures });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ctx: &Context,
        _dimension: Dimension,
        prefs: &mut DisplayPrefs,
        show_ores: &mut bool,
        show_structures: &mut bool,
        ore_cache: &mut ore_icons::IconCache,
        structure_cache: &mut structure_icons::IconCache,
        ores: &[Entry],
        structures: &[Entry],
    ) {
        self.set_ores_mode(prefs.feature_bar_ores);
        egui::SidePanel::right("feature-icon-rail")
            .resizable(false)
            .exact_width(46.)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(4, 8)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().button_padding = Vec2::splat(2.);
                ui.spacing_mut().item_spacing = Vec2::splat(2.);
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.vertical(|ui| {
                        let icon = structure_cache.get(ctx, "minecraft:village");
                        let response = ui.add_sized(
                            Vec2::splat(34.),
                            egui::Button::image(egui::Image::new((icon.id(), Vec2::splat(28.))))
                                .selected(self.group == Mode::Structures),
                        );
                        if response.on_hover_text("Structures").clicked() {
                            self.group = Mode::Structures;
                            prefs.feature_bar_ores = false;
                        }
                        let response = if let Some(icon) = ore_cache.get(ctx, "normal_gold") {
                            ui.add_sized(
                                Vec2::splat(34.),
                                egui::Button::image(egui::Image::new((
                                    icon.id(),
                                    Vec2::splat(28.),
                                )))
                                .selected(self.group == Mode::Ores),
                            )
                        } else {
                            draw_icon_button(ui, RailIcon::Unknown, "Ores")
                        };
                        if response.on_hover_text("Ores").clicked() {
                            self.group = Mode::Ores;
                            prefs.feature_bar_ores = true;
                        }
                    });
                    ui.separator();
                    let (kind, entries, globally_visible) = match self.group {
                        Mode::Structures => (MarkerKind::Structure, structures, *show_structures),
                        Mode::Ores => (MarkerKind::Ore, ores, *show_ores),
                    };
                    let rows: Vec<RailEntry> = match self.group {
                        Mode::Structures => structure_rows(entries),
                        Mode::Ores => ore_rows(entries),
                    };
                    if draw_icon_button(
                        ui,
                        RailIcon::Visibility(globally_visible),
                        "Toggle this feature group",
                    )
                    .clicked()
                    {
                        set_group_visible(show_ores, show_structures, &kind, !globally_visible);
                    }
                    let mut enable_group = false;
                    let group_ids: Vec<String> = rows
                        .iter()
                        .flat_map(|row| row.members.iter().cloned())
                        .collect();
                    ui.vertical(|ui| {
                        let visible = rows
                            .iter()
                            .filter(|row| row.members.iter().any(|id| !is_hidden(prefs, &kind, id)))
                            .count();
                        let total = rows.len();
                        if draw_icon_button(
                            ui,
                            RailIcon::All,
                            &format!("Show all types · {visible}/{total} visible"),
                        )
                        .clicked()
                        {
                            prefs.set_feature_group_visible(&kind, &group_ids, true);
                            enable_group = true;
                        }
                        if draw_icon_button(
                            ui,
                            RailIcon::None,
                            &format!("Hide all types · {visible}/{total} visible"),
                        )
                        .clicked()
                        {
                            prefs.set_feature_group_visible(&kind, &group_ids, false);
                        }
                        if draw_icon_button(
                            ui,
                            RailIcon::Reset,
                            &format!("Reset type filters · {visible}/{total} visible"),
                        )
                        .clicked()
                        {
                            prefs.set_feature_group_visible(&kind, &group_ids, true);
                            enable_group = true;
                        }
                    });
                    if enable_group {
                        set_group_visible(show_ores, show_structures, &kind, true);
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            for entry in &rows {
                                let visible_count = entry
                                    .members
                                    .iter()
                                    .filter(|id| !is_hidden(prefs, &kind, id))
                                    .count();
                                let visible = visible_count > 0;
                                let response = match kind {
                                    MarkerKind::Ore => {
                                        ore_cache.get(ctx, &entry.icon_id).map(|texture| {
                                            let tint = if visible {
                                                egui::Color32::WHITE
                                            } else {
                                                egui::Color32::from_gray(105)
                                            };
                                            ui.add_sized(
                                                Vec2::splat(34.),
                                                egui::Button::image(
                                                    egui::Image::new((
                                                        texture.id(),
                                                        Vec2::splat(29.),
                                                    ))
                                                    .tint(tint),
                                                )
                                                .selected(visible),
                                            )
                                        })
                                    }
                                    MarkerKind::Structure => Some({
                                        let texture = structure_cache.get(ctx, &entry.icon_id);
                                        let tint = if visible {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::from_gray(105)
                                        };
                                        ui.add_sized(
                                            Vec2::splat(34.),
                                            egui::Button::image(
                                                egui::Image::new((texture.id(), Vec2::splat(29.)))
                                                    .tint(tint),
                                            )
                                            .selected(visible),
                                        )
                                    }),
                                    _ => None,
                                };
                                let response = response.unwrap_or_else(|| {
                                    draw_icon_button(ui, RailIcon::Unknown, &entry.label)
                                });
                                let response = response.on_hover_text(format!(
                                    "{}{}{}",
                                    entry.label,
                                    if entry.members.len() > 1 {
                                        format!(
                                            " · {} variants · right-click to {}",
                                            entry.members.len(),
                                            if self.expanded.contains(&entry.key) {
                                                "collapse"
                                            } else {
                                                "expand"
                                            }
                                        )
                                    } else {
                                        String::new()
                                    },
                                    if globally_visible {
                                        ""
                                    } else {
                                        " · group hidden"
                                    }
                                ));
                                if response.clicked() {
                                    let shift = ui.input(|i| i.modifiers.shift);
                                    if shift {
                                        prefs.set_feature_group_visible(&kind, &group_ids, false);
                                        prefs.set_feature_group_visible(
                                            &kind,
                                            &entry.members,
                                            true,
                                        );
                                    } else {
                                        prefs.set_feature_group_visible(
                                            &kind,
                                            &entry.members,
                                            !visible,
                                        );
                                    }
                                    if !globally_visible {
                                        set_group_visible(show_ores, show_structures, &kind, true);
                                    }
                                }
                                if response.secondary_clicked() {
                                    if entry.members.len() > 1 {
                                        if !self.expanded.insert(entry.key.clone()) {
                                            self.expanded.remove(&entry.key);
                                        }
                                    } else {
                                        prefs.solo_feature(
                                            &kind,
                                            rows.iter().flat_map(|row| row.members.iter().cloned()),
                                            &entry.icon_id,
                                        );
                                        if !globally_visible {
                                            set_group_visible(
                                                show_ores,
                                                show_structures,
                                                &kind,
                                                true,
                                            );
                                        }
                                    }
                                }
                                if !visible {
                                    let rect = response.rect;
                                    ui.painter().rect_filled(
                                        rect,
                                        2.,
                                        egui::Color32::from_black_alpha(112),
                                    );
                                }
                                if entry.members.len() > 1 && self.expanded.contains(&entry.key) {
                                    for member in &entry.members {
                                        let member_entry = entries.iter().find(|e| &e.id == member);
                                        let label = member_entry
                                            .map(|e| e.label.as_str())
                                            .unwrap_or(member);
                                        let hidden = is_hidden(prefs, &kind, member);
                                        ui.horizontal(|ui| {
                                            ui.add_space(3.);
                                            let texture = match kind {
                                                MarkerKind::Ore => ore_cache
                                                    .get(ctx, member)
                                                    .map(|texture| texture.id()),
                                                _ => Some(structure_cache.get(ctx, member).id()),
                                            };
                                            let tint = if hidden {
                                                egui::Color32::from_gray(105)
                                            } else {
                                                egui::Color32::WHITE
                                            };
                                            let child = if let Some(texture) = texture {
                                                ui.add_sized(
                                                    Vec2::splat(31.),
                                                    egui::Button::image(
                                                        egui::Image::new((
                                                            texture,
                                                            Vec2::splat(25.),
                                                        ))
                                                        .tint(tint),
                                                    )
                                                    .selected(!hidden),
                                                )
                                            } else {
                                                draw_icon_button(ui, RailIcon::Unknown, label)
                                            }
                                            .on_hover_text(format!("{label} · {member}"));
                                            if child.clicked() {
                                                if ui.input(|i| i.modifiers.shift) {
                                                    prefs.solo_feature(
                                                        &kind,
                                                        rows.iter().flat_map(|row| {
                                                            row.members.iter().cloned()
                                                        }),
                                                        member,
                                                    );
                                                } else {
                                                    prefs
                                                        .set_feature_visible(&kind, member, hidden);
                                                }
                                                if !globally_visible {
                                                    set_group_visible(
                                                        show_ores,
                                                        show_structures,
                                                        &kind,
                                                        true,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                        });
                });
            });
        prefs.feature_bar_ores = self.ores_mode();
    }
}

fn set_group_visible(ores: &mut bool, structures: &mut bool, kind: &MarkerKind, visible: bool) {
    match kind {
        MarkerKind::Ore => *ores = visible,
        MarkerKind::Structure => *structures = visible,
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum RailIcon {
    Visibility(bool),
    All,
    None,
    Reset,
    Unknown,
}

fn draw_icon_button(ui: &mut egui::Ui, icon: RailIcon, tip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(19.), Sense::click());
    let selected = matches!(icon, RailIcon::Visibility(true));
    let fill = if response.hovered() || selected {
        egui::Color32::from_rgb(38, 67, 72)
    } else {
        egui::Color32::from_rgb(27, 39, 46)
    };
    ui.painter().rect_filled(rect, 4., fill);
    let stroke = egui::Stroke::new(
        1.4_f32,
        if selected {
            egui::Color32::from_rgb(116, 224, 200)
        } else {
            egui::Color32::from_rgb(194, 211, 210)
        },
    );
    let c = rect.center();
    let p = |x: f32, y: f32| egui::pos2(c.x + x, c.y + y);
    let eye = matches!(
        icon,
        RailIcon::Visibility(_) | RailIcon::All | RailIcon::None
    );
    if eye {
        let points = [
            p(-7., 0.),
            p(-3., -4.5),
            p(3., -4.5),
            p(7., 0.),
            p(3., 4.5),
            p(-3., 4.5),
            p(-7., 0.),
        ];
        for pair in points.windows(2) {
            ui.painter().line_segment([pair[0], pair[1]], stroke);
        }
        ui.painter().circle_filled(c, 1.8, stroke.color);
        if matches!(icon, RailIcon::None | RailIcon::Visibility(false)) {
            ui.painter().line_segment(
                [p(-7., 7.), p(7., -7.)],
                egui::Stroke::new(1.8_f32, stroke.color),
            );
        }
    } else if matches!(icon, RailIcon::Reset) {
        let points = [
            p(5., -6.),
            p(7., -1.),
            p(6., 4.),
            p(2., 7.),
            p(-3., 7.),
            p(-7., 3.),
            p(-7., -2.),
            p(-4., -6.),
        ];
        for pair in points.windows(2) {
            ui.painter().line_segment([pair[0], pair[1]], stroke);
        }
        ui.painter().line_segment([p(5., -6.), p(1., -6.)], stroke);
        ui.painter().line_segment([p(5., -6.), p(5., -2.)], stroke);
    } else {
        let points = [p(0., -7.), p(7., 0.), p(0., 7.), p(-7., 0.), p(0., -7.)];
        for pair in points.windows(2) {
            ui.painter().line_segment([pair[0], pair[1]], stroke);
        }
        ui.painter().circle_filled(c, 1.8, stroke.color);
    }
    response.on_hover_text(tip)
}

fn is_hidden(prefs: &DisplayPrefs, kind: &MarkerKind, id: &str) -> bool {
    match kind {
        MarkerKind::Ore => prefs.hidden_ore_ids.iter().any(|hidden| hidden == id),
        MarkerKind::Structure => prefs.hidden_structure_ids.iter().any(|hidden| hidden == id),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.into(),
            label: id.replace('_', " "),
        }
    }

    #[test]
    fn ore_families_use_actual_outputs_and_preserve_each_vein_control() {
        let entries = [
            "normal_copper",
            "normal_copper_gypsum",
            "surface_native_copper",
            "high_realgar",
            "deep_copper_sulfide",
            "high_sulfur",
            "normal_coal",
            "surface_coal",
            "high_coal",
            "sand_tin",
            "deep_asbestos",
            "high_tin",
            "normal_gold",
            "surface_gold",
        ]
        .map(entry);
        let rows = ore_rows(&entries);
        let copper = rows.iter().find(|row| row.label == "copper").unwrap();
        assert_eq!(
            copper.members,
            [
                "high_realgar",
                "normal_copper",
                "normal_copper_gypsum",
                "surface_native_copper"
            ]
        );
        assert_eq!(copper.icon_id, "normal_copper");
        // This mixed vein's main output is sulfur despite its legacy name.
        let sulfur = rows.iter().find(|row| row.label == "sulfur").unwrap();
        assert_eq!(sulfur.members, ["deep_copper_sulfide", "high_sulfur"]);
        assert_eq!(
            rows.iter()
                .find(|row| row.label == "coal")
                .unwrap()
                .members
                .len(),
            3
        );
        assert_eq!(
            rows.iter().find(|row| row.label == "tin").unwrap().members,
            ["deep_asbestos", "high_tin", "sand_tin"]
        );
        let mut prefs = DisplayPrefs::default();
        prefs.set_feature_group_visible(&MarkerKind::Ore, &copper.members, false);
        prefs.set_feature_visible(&MarkerKind::Ore, "normal_copper", true);
        assert!(!is_hidden(&prefs, &MarkerKind::Ore, "normal_copper"));
        assert!(is_hidden(&prefs, &MarkerKind::Ore, "surface_native_copper"));
        assert!(!is_hidden(&prefs, &MarkerKind::Ore, "normal_gold"));
    }

    #[test]
    fn ore_groups_partition_real_catalog_and_keep_dimensions_and_unknown_ids_distinct() {
        let (ores, _) = super::super::workspace::feature_catalog();
        let entries: Vec<_> = ores
            .iter()
            .map(|(id, label)| Entry {
                id: id.clone(),
                label: label.clone(),
            })
            .collect();
        let rows = ore_rows(&entries);
        let mut members: Vec<_> = rows
            .iter()
            .flat_map(|row| row.members.iter().cloned())
            .collect();
        members.sort();
        let mut expected: Vec<_> = entries.iter().map(|entry| entry.id.clone()).collect();
        expected.sort();
        assert_eq!(
            members, expected,
            "every vein remains selectable exactly once"
        );
        assert!(rows.len() < entries.len() * 3 / 4);
        for row in &rows {
            assert!(
                crate::ore_icons::icon_png(&row.icon_id).is_some(),
                "{}",
                row.icon_id
            );
            let beneath = row.members[0].starts_with("beneath_");
            assert!(
                row.members
                    .iter()
                    .all(|id| id.starts_with("beneath_") == beneath)
            );
        }
        let mut reversed = entries.clone();
        reversed.reverse();
        let identity = |rows: Vec<RailEntry>| {
            rows.into_iter()
                .map(|row| (row.key, row.icon_id, row.members))
                .collect::<Vec<_>>()
        };
        assert_eq!(identity(rows), identity(ore_rows(&reversed)));
        let unknown = ore_rows(&[entry("custom:normal_copper"), entry("other:normal_copper")]);
        assert_eq!(unknown.len(), 2);
    }

    #[test]
    fn ruin_church_and_building_families_cover_variants_without_swallowing_other_objects() {
        let entries = [
            "tfc_ruins:ruins",
            "tfc_ruins:ruin_small",
            "tfc_ruins:ruin_beach",
            "tfc_ruins:ruin_rich",
            "other:ruin_small",
            "tfc_ruined_world:limestone_church",
            "tfc_ruined_world:small_limestone_church_2",
            "tfc_ruined_world:medium_gneiss_church",
            "tfc_ruined_world:towerhouse_1",
            "tfc_ruined_world:tower_1",
            "tfc_ruined_world:castle_1",
            "wan:desert_ruins",
            "wan:desert_ruins_small",
            "wan:ruined_castle",
            "wan:ice_spike_castle",
            "wan:abandoned_village",
            "wan:mesa_temple",
            "wan:mesa_temple_small",
        ]
        .map(entry);
        let rows = structure_rows(&entries);
        let group = |id: &str| {
            rows.iter()
                .find(|row| row.members.iter().any(|member| member == id))
                .unwrap()
        };
        assert_eq!(group("tfc_ruins:ruins").members.len(), 4);
        assert_eq!(group("other:ruin_small").members.len(), 1);
        assert_eq!(group("tfc_ruined_world:limestone_church").members.len(), 3);
        assert_ne!(
            group("tfc_ruined_world:towerhouse_1").key,
            group("tfc_ruined_world:tower_1").key
        );
        assert_ne!(
            group("tfc_ruined_world:towerhouse_1").key,
            group("tfc_ruined_world:castle_1").key
        );
        assert_eq!(group("wan:desert_ruins").members.len(), 2);
        assert_eq!(
            group("wan:ruined_castle").members,
            ["wan:ice_spike_castle", "wan:ruined_castle"]
        );
        assert_eq!(group("wan:abandoned_village").members.len(), 1);
        assert_eq!(group("wan:mesa_temple").members.len(), 2);
    }

    #[test]
    fn preference_helpers_toggle_solo_and_restore_types_independently() {
        let mut prefs = DisplayPrefs::default();
        let ores = vec!["normal_gold".to_owned(), "normal_iron".to_owned()];
        let structures = vec![
            "minecraft:village".to_owned(),
            "tfg:beneath/tower".to_owned(),
        ];
        prefs.set_feature_visible(&MarkerKind::Ore, "normal_gold@-4,8", false);
        assert_eq!(prefs.hidden_ore_ids, ["normal_gold"]);
        prefs.solo_feature(&MarkerKind::Ore, ores.clone(), "normal_iron");
        assert_eq!(prefs.hidden_ore_ids, ["normal_gold"]);
        prefs.set_feature_group_visible(&MarkerKind::Ore, &ores, true);
        assert!(prefs.hidden_ore_ids.is_empty());
        prefs.set_feature_group_visible(&MarkerKind::Structure, &structures, false);
        prefs.solo_feature(&MarkerKind::Structure, structures, "tfg:beneath/tower");
        assert_eq!(prefs.hidden_structure_ids, ["minecraft:village"]);
        assert!(
            prefs.hidden_ore_ids.is_empty(),
            "group filters remain independent"
        );
    }

    #[test]
    fn feature_bar_mode_round_trips_through_display_preferences() {
        let mut state = State::default();
        state.set_ores_mode(true);
        assert!(state.ores_mode());
        let prefs = DisplayPrefs {
            feature_bar_ores: state.ores_mode(),
            ..Default::default()
        };
        let restored =
            serde_json::from_str::<DisplayPrefs>(&serde_json::to_string(&prefs).unwrap()).unwrap();
        state.set_ores_mode(restored.feature_bar_ores);
        assert!(state.ores_mode());
    }

    #[test]
    fn structure_variants_group_without_merging_namespaces_or_ore_types() {
        let structures = vec![
            Entry {
                id: "mod:tower_1".into(),
                label: "Tower 1".into(),
            },
            Entry {
                id: "mod:tower_2".into(),
                label: "Tower 2".into(),
            },
            Entry {
                id: "other:tower_1".into(),
                label: "Tower 1".into(),
            },
            Entry {
                id: "species:digsite_small_desert".into(),
                label: "Small desert digsite".into(),
            },
            Entry {
                id: "species:digsite_large_desert".into(),
                label: "Large desert digsite".into(),
            },
            Entry {
                id: "minecraft:desert_temple".into(),
                label: "Desert temple".into(),
            },
            Entry {
                id: "alekiships:unfinished_sloop_oak".into(),
                label: "Unfinished sloop oak".into(),
            },
            Entry {
                id: "alekiships:unfinished_sloop_birch".into(),
                label: "Unfinished sloop birch".into(),
            },
            Entry {
                id: "alekiships:unfinished_rowboat_oak".into(),
                label: "Unfinished rowboat oak".into(),
            },
            Entry {
                id: "wan_ancient_beasts:abandoned_village".into(),
                label: "Abandoned village".into(),
            },
            Entry {
                id: "tfg:illagers/arabic_village".into(),
                label: "Arabic village".into(),
            },
            Entry {
                id: "tfg:illagers/arabic_village_living".into(),
                label: "Arabic village living".into(),
            },
            Entry {
                id: "tfg:illagers/norse_village_ruined".into(),
                label: "Norse village ruined".into(),
            },
            Entry {
                id: "wan_ancient_beasts:training_outpost".into(),
                label: "Training outpost".into(),
            },
        ];
        let groups = structure_rows(&structures);
        let tower = groups.iter().find(|g| g.label == "Towers").unwrap();
        assert_eq!(tower.members, ["mod:tower_1", "mod:tower_2"]);
        assert!(groups.iter().any(|g| g.members == ["other:tower_1"]));
        let digsites = groups
            .iter()
            .find(|g| {
                g.members
                    .iter()
                    .any(|id| id.starts_with("species:digsite_"))
            })
            .unwrap();
        assert_eq!(digsites.members.len(), 2);
        assert!(
            groups
                .iter()
                .any(|g| g.members == ["minecraft:desert_temple"])
        );
        let sloop = groups
            .iter()
            .find(|g| g.label == "Unfinished sloops")
            .unwrap();
        assert_eq!(sloop.members.len(), 2);
        let rowboat = groups
            .iter()
            .find(|g| g.label == "Unfinished rowboats")
            .unwrap();
        assert_eq!(rowboat.members, ["alekiships:unfinished_rowboat_oak"]);
        let village = groups
            .iter()
            .find(|g| {
                g.members
                    .contains(&"wan_ancient_beasts:abandoned_village".to_owned())
            })
            .unwrap();
        assert_eq!(village.members, ["wan_ancient_beasts:abandoned_village"]);
        let tfg_villages = groups
            .iter()
            .find(|g| {
                g.members
                    .contains(&"tfg:illagers/arabic_village".to_owned())
            })
            .unwrap();
        assert_eq!(tfg_villages.members.len(), 3);
        assert!(
            groups
                .iter()
                .any(|g| g.members == ["wan_ancient_beasts:training_outpost"])
        );

        let mut prefs = DisplayPrefs::default();
        prefs.set_feature_group_visible(&MarkerKind::Structure, &tower.members, false);
        assert!(
            tower
                .members
                .iter()
                .all(|id| is_hidden(&prefs, &MarkerKind::Structure, id))
        );
        prefs.set_feature_visible(&MarkerKind::Structure, &tower.members[0], true);
        assert!(!is_hidden(
            &prefs,
            &MarkerKind::Structure,
            &tower.members[0]
        ));
        assert!(is_hidden(&prefs, &MarkerKind::Structure, &tower.members[1]));
        prefs.set_feature_group_visible(&MarkerKind::Structure, &tower.members, true);
        assert!(
            tower
                .members
                .iter()
                .all(|id| !is_hidden(&prefs, &MarkerKind::Structure, id))
        );
    }
}
