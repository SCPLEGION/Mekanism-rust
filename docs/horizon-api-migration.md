# Mekanism: from the fork's `native_*` hooks to the upstream mod API

Stage 1 of the move. Source of the old side: `mixins/mekanism/horizon-mixin.toml` (75 injections, all on hooks of
the fork `legacy/fork-hooks-api93`) and the 1267 `native_*` uses in `mixins/mekanism/runtime.rs` (35k lines, a module
inside the game crate). New side: `horizon-mod-api` / `horizon-mod-vocab` of upstream (`SCPLEGION/horizon` main =
upstream + helpers #90/#91). Names of upstream hooks follow NeoForge 26.3, so where the Java mod used an event,
the same name is the first place to look.

## Target shape

One native mod crate `mods/mekanism` (like JEI's `mods/jei`): `horizon-mod.toml` + `Cargo.toml` + `src/lib.rs`
with `#[mod_api::horizon_mod] impl Hooks`. The pure logic crate `mixins/mekanism/core` (68k lines, no game types)
stays as it is and becomes a dependency of the mod. `runtime.rs` is the part that is rewritten: it is split by
feature into files of `mods/mekanism/src/`, and every `game.native_*` call becomes a call on a `Host`
(`ServerHost`/`ClientHost`) handle. Escape hatch for what no hook offers: `[[native.mixin]]`, `[[native.module]]`
(the same idea as the old bundle, but declared in `horizon-mod.toml`).

Content (blocks, items, recipes, models, textures) already lives in the four `.hmod` content mods built by
`tools/carry_content.py`; it is unchanged.

## 1. Hooks (the 75 injections and the 20 others of the fork)

"Equal" = same moment, port the body. "Part" = upstream puts it on a Part trait registered in `Hooks::register`
instead of a global hook. "Mixin" = no hook, use `[[native.mixin]]` (last resort).

| Legacy hook | Upstream | Note |
|---|---|---|
| `frame`, `tick` | `client_tick_post`, `render_frame_pre` | jetpack thrust, HUD state |
| `server_tick`, `mobs_tick` | `server_tick`, `level_tick_post`, `entity_tick_pre` | machines tick as block entities, see below |
| `world_lifecycle` | `world_lifecycle` (also `level_load`, `level_unload`, `server_starting`) | equal |
| `block_changed`, `neighbor_changed` | `block_changed`, `neighbor_changed`, `on_neighbor_change` | equal |
| `hud` | `register_gui_layers` + `gui_layer_draw` | HUD layers (radiation, module HUD) |
| `key`, `wheel` | `input_key`, `mouse_scrolling`, `register_key_mappings` (Part `key_mappings`) | keys become real key mappings |
| `command`, `register_commands`, `perform_command` | `register_commands`, `register_client_commands`, `run_mod_command` | Brigadier-based |
| `zoom`, `minimap` | `zoom`, `minimap` | equal |
| `mods`, `settings`, `settings_click`, `panel_event` | `mod`, Part `settings`, `setting_changed`, `panel_event` | config screens through `mod_configs` |
| `block_entity` | Part `block_entities` (`BlockEntityPart`) + hook `block_entity` | state in NBT or an attachment |
| `block_entity_ticks`, `block_entity_tick` | `block_entity_ticks`, `block_entity_tick` (`BlockEntityPart::tick`) | equal |
| `block_entity_container` | `r.block_container(&block, ModContainer)` | slots for hoppers and pipes |
| `client_block_entity_changed` | `block_entity_data_received`, `block_entity_update_tag` | equal |
| `screen`, `screen_draw`, `screen_click`, `screen_tooltip`, `screen_overlay`, `screen_covers`, `screen_typing`, `screen_key`, `screen_alt`, `screen_escape`, `screen_wheel`, `screen_drop` | Part `menus` (`MenuPart`) with `mod_screen`; hooks `screen_render_post`, `screen_mouse_button_pressed_pre`, `screen_key_pressed_pre`, `screen_char_typed_pre`, `screen_mouse_scrolled_pre`, `screen_tooltip`, `screen_closing`, `screen_room`, `screen_exclusion_areas` | the biggest block: GUI fidelity lives here |
| `container_draw/click/key/wheel/release` | the same `screen_*` hooks on the vanilla container screens, `screen_room` for side panels | JEI-style side panel |
| `ui_draw/click/key/wheel/closed` | `mod_screen_open`/`mod_screen_close` + widgets (`host/draw.rs`: `widget_button`, `widget_text_field`) | |
| `slot_rules`, `slot_rules_at` | `menu_slot_rules`, `menu_slot_backing`, `menu_quick_move` | equal |
| `menu_button`, `menu_clicked`, `menu_closed` | `menu_button`, `menu_clicked`, `menu_closed` | equal |
| `creative_tabs` | `build_creative_mode_tab_contents` | equal |
| `tooltip` | `item_tooltip`, `add_attribute_tooltips`, `render_tooltip_gather_components` | |
| `use_item_on` | `block_use_item_on`, `use_item_on_block`, `on_item_use_first` | one per kind of claim |
| `use_item`, `using_tick` | `use_item`, `on_use_tick`, `living_entity_use_item_tick` | |
| `destroy_block`, `place_block`, `block_removed`, `right_click_block` | `break_block`, `player_destroyed_block`, `on_block_placed_by`, `remove_block`, `on_destroyed_by_player`, `right_click_block` | |
| `block_redirect` | `block_clone_item` / `block_use_without_item`, else Mixin | check when porting |
| `destroy_speed` | `player_break_speed` | equal |
| `player_attack`, `player_damage`, `player_damage_rules`, `player_immunities` | `attack_entity`, `critical_hit`, `living_damage_pre`, `entity_invulnerability_check` | |
| `gear_attributes`, `attack_speed` | `item_attribute_modifier` | |
| `glider`, `glide_wear`, `may_fly` | `entity_toggle_glide`, `player_toggle_flight`, Part `items` (equippable glider) | |
| `bow_item`, `bow_shot`, `drink_item` | `arrow_nock`, `arrow_loose`, `living_get_projectile`, `use_animation`, `use_duration` | |
| `acts_as_shears`, `shears_used`, `shears_trim` | `can_perform_action`, `block_shear_entity`, `damage_item` | |
| `item_bars` | `item_bar` | equal |
| `worn_tint` | `item_tint_source`, `register_item_tint_sources` | |
| `client_payload`, `server_payload` | Part `payloads` (typed channels), `player_send_payload`, `send_payload_to_server` | |
| `world_boxes` | `block_highlight`, `debug_renderer_draw` | |
| `world_models`, `world_items`, `world_texts`, `world_quads` | `add_section_geometry`, `submit_custom_geometry`, `render_level_stage`, `register_special_model_renderer`, `draw_scene` | |
| `fog` | `render_fog`, `compute_fog_color` | |
| `mob_types` | Part `entity_types` + `entity_renderers`, `register_entity_layers` | Robit, flame mobs |
| `mob_died`, `use_mob`, `mob_hidden` | `living_death`, `entity_interact`, `render_living_pre` (cancel) | |
| `biome_spawns`, `worldgen_entries` | `native_potential_spawns`, `mob_spawn_checks`, Part `biome_modifiers`, `structures`, `datapack_registries` | ore veins are data |
| `dispense`, `projectile_moved` | `dispense_item`, `dispense_method`, `projectile_launch`, `entity_tick_pre` | |

## 2. Services the runtime calls on `Game` (`game.native_*`)

Count in `runtime.rs` in brackets. Everything has an equal in `Host`, except the rows marked GAP.

| Legacy call | Upstream |
|---|---|
| `with_mod_block_entity` [132], `client_block_entity` [40], `take/put_block_entity`, `sync_block_entity` | `Host::block_entity`, `block_entity_data`, `block_entity_replace_data`, `block_entity_sync`; per-entity data as attachments |
| `on` [127] (is mod X on) | `Host::on` |
| `text`, `text_width`, `text_scaled`, `fill`, `fill_corners`, `fill_quad`, `sprite`, `sprite_tinted`, `item`, `item_stack`, `gui_player` [~390] | `host/draw.rs`: `text`, `text_width`, `fill`, `fill_gradient`, `sprite`, `tiled_sprite`, `gui_nine_slice`, `item_stack`, `gui_entity`, `push/pop`, `scale`, `translate` |
| `cursor`, `mouse_down`, `clicks`, `mapping`, `mod_key` | `client_input.rs`: `cursor`, `is_down`, `click_mapping`, `mapping`, `mod_key` |
| `send_to_server`, `send_to_player` [72] | `send_payload_to_server`, `player_send_payload` |
| `acting_player`, `player`, `server_players`, `online_players`, `player_by_name` | `services.rs`: `acting_player`, `players`, `player_by_name`; `players.rs` |
| `block_property`, `set_block_property`, `block`, `set_block`, `set_block_adjusted`, `block_properties`, `block_transform`, `face_sturdy`, `set_block_signal`, `has_neighbor_signal` | `block_states.rs`, `levels.rs`: `level_block_state`, `set_block_property`, `level_block_transform`, `level_block_face_sturdy`, `level_best_neighbor_signal` |
| `with_mob`, `mob_ids`, `entities_in`, `hurt_mob`, `mob_pose`, `kill` | `entities.rs`, `mobs.rs`, `living.rs`: `entity_*`, `living_*`, `mob_*` |
| `insert_item`, `extract_item(_where)`, `can_insert_item` | capability `ITEMS` lookup (`level.capability`), `game_insert`, `game_extract`, `container_*` |
| `drop_item`, `take_item_entity`, `give_items`, `world_items` | `drop_at`, `item_entities.rs`, `give` |
| `loop_sound`, `stop_loop_sound` | `effects.rs`: `play_local_sound`, `sound_state`, `sound_stop` |
| `add_particle`, `add_dust` | `add_particle`, particle Part |
| `setting`, `set_setting`, `game_rule`, `world_dir`, `data_dir` | `config.rs`: `setting*`, `game_rule`, `world_dir`, `config_files` |
| `screen_title`, `open_screen_block`, `ui_open`, `open_ui`, `close_ui` | `screens.rs`: `screen_title`, `mod_screen_open`, `mod_screen_close`, `observed_screen_id` |
| `action_bar`, `tell`, `chat` | `player_action_bar`, `feedback`/`messages`, `chat_line` |
| `explode`, `clip`, `eye`, `camera_axes`, `start_using`, `timed_effects`, `speed_up_effect` | `levels.rs` (`level_clip`, explosions), `entity_eye_position`, `client_view.rs` `camera`, `living_*` use and effects |
| `keep_chunks` | `chunk_set_mod_forced` |
| `gui_model` | `draw.rs`: `item_model`, `draw_scene`, `model_part` |
| fluid and energy ports | capabilities `ENERGY`, `FLUIDS`, `ITEMS` (`fluids_energy.rs`); Mekanism's chemicals and heat: own capability (`concepts/capabilities/own-capability.md`) |
| recipes, `known_recipes`, `place_recipe` | `recipes.rs` |
| `game.rs` `native_hooks` module (74 uses) | the module itself disappears with the fork |

## 3. Gaps (expected: small generic helpers on the fork's main, step 2)

Not yet verified against upstream code, to confirm while porting each stage; each becomes a small PR on
`SCPLEGION/horizon` only if the check shows a real gap:

1. Chemical (gas, infusion, pigment, slurry) handler type: use an own capability; check that pipes of other mods are not needed.
2. `world_boxes` (the digital miner's and the quarry's outlines): `block_highlight` may not cover arbitrary boxes.
3. `block_redirect` (a block taking a click for another, bounding blocks): maybe `block_use_without_item` plus a mixin.
4. 64-bit menu data (energy up to 2^63): menus carry `i16` data slots; keep the old route (payload channel).
5. GUI sprites at fractional positions and nine-slice with the original's exact borders (`gui_nine_slice` should cover it).
6. Creative tab of a content `.hmod` item with components (module and upgrade items).

## 4. Build

`tools/manual_stage.py` stages the old bundle into a game checkout and read `native-hooks.json`: obsolete.
Replacement: `tools/build_native.py` runs `cargo horizon-mod check mods/mekanism` and
`horizon-launcher native-build mods/mekanism --trust-native-code`, with `[native] horizon` pinned to the fork's
main (see `JustEnoughItems-rust/mods/jei/horizon-mod.toml`; local fork build: `developer-install --trust-unsigned`).
