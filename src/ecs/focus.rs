use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::{Add, Remove};
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Has, With};
use bevy::ecs::system::{Commands, Populated, Query, Res, Single};
use bevy::math::{IRect, IVec2};
use bevy::prelude::Event as BevyEvent;
use tracing::{Level, debug, instrument, trace, warn};

use super::{FocusedMarker, MouseHeldMarker, PendingMouseWarp, RepositionMarker, SystemTheme};
use crate::config::Config;
use crate::ecs::layout::LayoutStrip;
use crate::ecs::params::{ActiveDisplay, ActiveDisplayMut, Configuration, Windows};
use crate::ecs::{
    ActiveWorkspaceMarker, Scrolling, SelectedVirtualMarker, reposition_entity, reshuffle_around,
};
use crate::manager::{Application, Window, WindowManager};

#[derive(BevyEvent)]
pub(super) struct FocusWindow {
    pub entity: Entity,
    pub raise: bool,
}

#[allow(clippy::needless_pass_by_value)]
#[instrument(level = Level::DEBUG, skip_all, fields(trigger))]
pub(super) fn maintain_focus_singleton(
    trigger: On<Add, FocusedMarker>,
    windows: Query<(Entity, Has<FocusedMarker>), With<Window>>,
    mut active_display: ActiveDisplayMut,
    mut config: Configuration,
    mut commands: Commands,
) {
    let focused_entity = trigger.event().entity;

    for (entity, focused) in windows {
        if focused
            && entity != focused_entity
            && let Ok(mut entity_commands) = commands.get_entity(entity)
        {
            debug!("window {entity} lost focus.");
            entity_commands.try_remove::<FocusedMarker>();
        }
    }

    // Update last_focused on the column so accordion mode can
    // restore the correct focused_index when focus returns.
    let strip = active_display.active_strip();
    if let Ok(index) = strip.index_of(focused_entity)
        && let Some(column) = strip.get_column_mut(index)
    {
        column.move_to_front(focused_entity);
    }

    // Check if the reshuffle was caused by a keyboard switch or mouse move.
    // Skip reshuffle if caused by mouse - because then it won't center.
    if config.ffm_flag().is_none() {
        config.set_skip_reshuffle(false);
    }
    config.set_ffm_flag(None);
}

#[allow(clippy::needless_pass_by_value)]
#[instrument(level = Level::DEBUG, skip_all, fields(trigger))]
pub(super) fn autocenter_window_on_focus(
    focused: Single<Entity, Added<FocusedMarker>>,
    mouse_held: Query<&MouseHeldMarker>,
    windows: Windows,
    config: Configuration,
    active_display: ActiveDisplay,
    mut commands: Commands,
) {
    let entity = *focused;

    if config.skip_reshuffle() || config.initializing() || !mouse_held.is_empty() {
        return;
    }
    if config.auto_center()
        && let Some((_, _, None)) = windows.get_managed(entity)
        && let Some(size) = windows.size(entity)
        && let Some(mut origin) = windows.origin(entity)
    {
        let center = active_display.bounds().center();
        origin.x = center.x - size.x / 2;
        reposition_entity(entity, origin, &mut commands);
    }
    reshuffle_around(entity, &mut commands);
}

#[allow(clippy::needless_pass_by_value)]
#[instrument(level = Level::DEBUG, skip_all, fields(trigger))]
pub(super) fn mouse_follows_focus(
    focused: Single<Entity, Added<FocusedMarker>>,
    windows: Windows,
    config: Configuration,
    active_workspace: Query<&Scrolling, With<ActiveWorkspaceMarker>>,
    mut commands: Commands,
) {
    let entity = *focused;
    let Some(window) = windows.get(entity) else {
        return;
    };
    if active_workspace
        .iter()
        .next()
        .is_some_and(|scrolling| scrolling.is_user_swiping)
    {
        debug!("Suppressing center mouse due to a swipe");
        return;
    }

    trace!(
        "window {}, skip_reshuffle {}, ffm flag {:?}.",
        window.id(),
        config.skip_reshuffle(),
        config.ffm_flag()
    );
    // Always defer: autocenter_window_on_focus calls reshuffle_around which
    // may reposition the strip, so the window's final position is not known
    // until the animation has finished. deferred_mouse_warp handles the
    // actual warp once all RepositionMarkers have been consumed.
    if config.mouse_follows_focus()
        && !config.skip_reshuffle()
        && config.ffm_flag().is_none_or(|id| id != window.id())
    {
        debug!("deferring mouse warp for {}", window.id());
        if let Ok(mut cmd) = commands.get_entity(entity) {
            cmd.try_insert(PendingMouseWarp);
        }
    }
}

/// Returns the warp target for a window frame on the given display, or `None`
/// if the window is not visible enough for a meaningful cursor placement.
///
/// When the window centre is inside the display the geometric centre is
/// returned directly. Otherwise the centre of the visible intersection is
/// used, provided the visible width is at least a third of the full window
/// width (avoids warping to a thin sliver at the display edge).
fn visible_warp_target(display: IRect, frame: IRect) -> Option<IVec2> {
    let visible = display.intersect(frame);
    if visible.height() <= 0 {
        return None;
    }
    if display.contains(frame.center()) {
        return Some(frame.center());
    }
    // Partly visible: require enough overlap to avoid edge-warping.
    if visible.width() > frame.width() / 3 {
        return Some(visible.center());
    }
    None
}

#[allow(clippy::needless_pass_by_value, clippy::type_complexity)]
#[instrument(level = Level::TRACE, skip_all)]
pub(super) fn deferred_mouse_warp(
    pending: Populated<
        (Entity, Has<RepositionMarker>),
        (With<PendingMouseWarp>, With<FocusedMarker>),
    >,
    active_workspace: Query<Has<RepositionMarker>, With<ActiveWorkspaceMarker>>,
    windows: Windows,
    config: Configuration,
    active_display: ActiveDisplay,
    window_manager: Res<WindowManager>,
    mut commands: Commands,
) {
    let strip_animating = active_workspace.iter().any(|animating| animating);

    for (entity, window_animating) in pending.iter() {
        if window_animating || strip_animating {
            continue;
        }
        let Some(frame) = windows.moving_frame(entity) else {
            continue;
        };
        if let Some(target) = visible_warp_target(active_display.bounds(), frame)
            && config.mouse_follows_focus()
        {
            debug!("deferred warp to {target}");
            window_manager.warp_mouse(target);
            if let Ok(mut cmd) = commands.get_entity(entity) {
                cmd.try_remove::<PendingMouseWarp>();
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn dim_window_trigger(
    trigger: On<Add, FocusedMarker>,
    windows: Windows,
    window_manager: Res<WindowManager>,
    config: Res<Config>,
    theme: Option<Res<SystemTheme>>,
) {
    let Some(window) = windows.get(trigger.event().entity) else {
        return;
    };

    let dark = theme.is_some_and(|theme| theme.is_dark);
    if config.window_dim_ratio(dark).is_some() {
        window_manager.dim_windows(&[window.id()], 0.0);
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn dim_remove_window_trigger(
    trigger: On<Remove, FocusedMarker>,
    windows: Windows,
    active_display: ActiveDisplay,
    window_manager: Res<WindowManager>,
    config: Res<Config>,
    theme: Option<Res<SystemTheme>>,
) {
    let Some(window) = windows.get(trigger.event().entity) else {
        return;
    };

    let same_display = active_display
        .active_strip()
        .contains(trigger.event().entity);
    if !same_display {
        // Do not dim the window loosing focus on another display.
        return;
    }

    let dark = theme.is_some_and(|theme| theme.is_dark);
    if let Some(dim_ratio) = config.window_dim_ratio(dark) {
        window_manager.dim_windows(&[window.id()], dim_ratio);
    }
}

#[allow(clippy::needless_pass_by_value)]
#[instrument(level = Level::DEBUG, skip_all, fields(trigger))]
pub(super) fn virtual_strip_activated(
    trigger: On<Add, FocusedMarker>,
    workspaces: Query<(Entity, &LayoutStrip, Has<ActiveWorkspaceMarker>)>,
    mut commands: Commands,
) {
    let owner_strip = workspaces.into_iter().find_map(|(entity, strip, active)| {
        (strip.contains(trigger.entity) && !active).then_some(entity)
    });
    if let Some(entity) = owner_strip
        && let Ok(mut entity_commands) = commands.get_entity(entity)
    {
        entity_commands
            .try_insert(ActiveWorkspaceMarker)
            .try_insert(SelectedVirtualMarker);
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn focus_window_trigger(
    trigger: On<FocusWindow>,
    windows: Windows,
    apps: Query<&Application>,
) {
    let FocusWindow { entity, raise } = *trigger.event();
    let Some(window) = windows.get(entity) else {
        return;
    };
    let Some(psn) = windows.psn(window.id(), &apps) else {
        return;
    };
    if !raise
        && let Some((focused_window, _)) = windows.focused()
        && let Some(focused_psn) = windows.psn(focused_window.id(), &apps)
    {
        window.focus_without_raise(psn, focused_window, focused_psn);
    } else {
        window.focus_with_raise(psn);
    }
}
