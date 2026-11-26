// SPDX-FileCopyrightText: 2026 The SCOURGE Project contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Additional terms: see the repository NOTICE and the LIVI attribution.

//! Normalize CarPlay CSM route guidance into the bridge's bounded v1 model.
//!
//! The source is `NavigationPatch`, produced by LIVI's iAP2 CSM decoder for
//! `0x5201` (route status/distances), `0x5202` (maneuver detail) and `0x5204`
//! (lane guidance). Missing values stay absent; they are never replaced by
//! zero. This module does not touch the vehicle bus.

use livi_core_proto::helper_events::NavigationPatch;
use serde::{Deserialize, Serialize};

pub const MAX_TEXT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteState {
    Active,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManeuverKind {
    Left,
    Right,
    Straight,
    UTurn,
    Roundabout,
    Arrive,
    KeepLeft,
    KeepRight,
    Merge,
    Exit,
    Ferry,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maneuver {
    /// Raw CSM maneuver type. Kept so the cluster adapter can choose an icon
    /// without depending on this crate's coarse `kind`.
    pub code: u8,
    pub kind: ManeuverKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteUpdate {
    pub state: RouteState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuver: Option<Maneuver>,
    /// Metres to the next maneuver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_m: Option<u32>,
    /// Metres to the destination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_m: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_road: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// Unix seconds; absent when the phone sends no usable ETA.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_remaining_s: Option<u32>,
}

impl RouteUpdate {
    pub fn clear() -> Self {
        Self {
            state: RouteState::Clear,
            maneuver: None,
            next_m: None,
            remaining_m: None,
            road: None,
            next_road: None,
            destination: None,
            eta_epoch: None,
            time_remaining_s: None,
        }
    }

    /// Returns `None` for a patch that carries no route change (for example a
    /// lane-only update or a duplicate without values).
    pub fn from_patch(patch: &NavigationPatch) -> Option<Self> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        Self::from_patch_at(patch, now)
    }

    /// Testable form of [`Self::from_patch`] with an explicit reference time
    /// for the ETA plausibility window.
    pub fn from_patch_at(patch: &NavigationPatch, now: u64) -> Option<Self> {
        if patch.status == Some(0) {
            return Some(Self::clear());
        }
        let maneuver = patch.turn.as_ref().and_then(|t| {
            t.maneuver_type.and_then(|code| {
                u8::try_from(code)
                    .ok()
                    .map(|code| Maneuver {
                        code,
                        kind: maneuver_kind(code),
                    })
            })
        });
        let next_m = metres(patch.remain_distance);
        let remaining_m = metres(patch.distance_to_destination);
        let road = text(patch.road_name.as_deref());
        let next_road = patch
            .turn
            .as_ref()
            .and_then(|t| text(t.after_road_name.as_deref()));
        let destination = text(patch.destination_name.as_deref());
        let eta_epoch = patch
            .eta_epoch
            .and_then(|value| eta_unix(value, now));
        let time_remaining_s = patch
            .time_to_destination
            .filter(|v| v.is_finite() && *v >= 0.0 && *v <= f64::from(u32::MAX))
            .map(|v| v.round() as u32);
        let update = Self {
            state: RouteState::Active,
            maneuver,
            next_m,
            remaining_m,
            road,
            next_road,
            destination,
            eta_epoch,
            time_remaining_s,
        };
        if update.maneuver.is_none()
            && update.next_m.is_none()
            && update.remaining_m.is_none()
            && update.road.is_none()
            && update.next_road.is_none()
            && update.destination.is_none()
            && update.eta_epoch.is_none()
            && update.time_remaining_s.is_none()
        {
            return None;
        }
        Some(update)
    }
}

/// CarPlay CSM sends `eta` as an unsigned 64-bit timestamp. Real phones have
/// been observed to use milliseconds since Apple's 2001 epoch; some send Unix
/// seconds. Accept only a candidate close to the current session, rather than
/// putting a wrong date on the cluster. The window allows a little clock skew
/// and long trips, but not a value from a different year.
fn eta_unix(value: f64, now: u64) -> Option<u64> {
    const APPLE_EPOCH: f64 = 978_307_200.0;
    const PAST: f64 = 3_600.0;
    const FUTURE: f64 = 30.0 * 86_400.0;
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 {
        return None;
    }
    let now = now as f64;
    let candidates = [value, value / 1000.0 + APPLE_EPOCH, value + APPLE_EPOCH];
    candidates
        .into_iter()
        .find(|v| *v >= now - PAST && *v <= now + FUTURE)
        .map(|v| v as u64)
}

fn metres(value: Option<f64>) -> Option<u32> {
    value
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= f64::from(u32::MAX))
        .map(|v| v.round() as u32)
}

fn text(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.chars().take(MAX_TEXT).collect())
}

pub fn maneuver_kind(code: u8) -> ManeuverKind {
    match code {
        1 | 20 | 47 | 49 => ManeuverKind::Left,
        2 | 21 | 48 | 50 => ManeuverKind::Right,
        3 | 5 => ManeuverKind::Straight,
        4 | 18 | 26 => ManeuverKind::UTurn,
        6 | 7 | 19 | 28..=46 => ManeuverKind::Roundabout,
        8 | 22 | 23 => ManeuverKind::Exit,
        9 | 51 | 52 | 53 => ManeuverKind::Merge,
        10 | 12 | 24 | 25 | 27 => ManeuverKind::Arrive,
        13 => ManeuverKind::KeepLeft,
        14 => ManeuverKind::KeepRight,
        15..=17 => ManeuverKind::Ferry,
        _ => ManeuverKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn patch(frame: &str) -> NavigationPatch {
        let mut tag = livi_helper_stack::events::EventTag::default();
        let event = tag
            .navigation_event(&hex(frame))
            .expect("route guidance event");
        match event {
            livi_core_proto::helper_events::HelperKind::Navigation(p) => *p,
            other => panic!("not navigation: {other:?}"),
        }
    }

    #[test]
    fn the_real_route_vector_keeps_both_distances_and_names() {
        let p = patch(
            "4040003c5201000600000014000500010100110003486175707473747261c39f6500000c0005000000b4e095638000080007000005dc0006000d0102",
        );
        let route = RouteUpdate::from_patch_at(&p, 1_755_000_000).unwrap();
        assert_eq!(route.state, RouteState::Active);
        assert_eq!(route.next_m, None, "the vector has no maneuver distance");
        assert_eq!(route.remaining_m, Some(1500));
        assert_eq!(route.road.as_deref(), Some("Hauptstraße"));
        assert!(route.destination.is_none());
        assert_eq!(route.eta_epoch, Some(1_755_169_200));
        assert_eq!(route.time_remaining_s, None);
        assert!(route.maneuver.is_none(), "0x5201 carries no turn detail");
    }

    #[test]
    fn the_maneuver_vector_maps_an_icon_code() {
        let mut tag = livi_helper_stack::events::EventTag::default();
        let route_frame =
            hex("4040003c5201000600000014000500010100110003486175707473747261c39f6500000c0005000000b4e095638000080007000005dc0006000d0102");
        tag.navigation_event(&route_frame).expect("route status");
        // The 0x5201 current list is one big-endian index, 0x0102 = 258.
        let maneuver_frame =
            hex("4040001f5202000600010102000500030500080004423237000006000bffa6");
        let Some(livi_core_proto::helper_events::HelperKind::Navigation(p)) =
            tag.navigation_event(&maneuver_frame)
        else {
            panic!("maneuver detail");
        };
        assert_eq!(p.turn.as_ref().unwrap().maneuver_type, Some(5));
        let route = RouteUpdate::from_patch(&p).unwrap();
        assert_eq!(route.state, RouteState::Active);
        assert_eq!(
            route.maneuver,
            Some(Maneuver {
                code: 5,
                kind: ManeuverKind::Straight
            })
        );
        assert_eq!(route.next_road.as_deref(), Some("B27"));
        assert_eq!(p.turn.as_ref().unwrap().turn_angle, Some(-90));
    }

    #[test]
    fn stopped_and_empty_patches_are_distinct() {
        let mut p = NavigationPatch::default();
        assert!(RouteUpdate::from_patch_at(&p, 1_755_000_000).is_none());
        p.status = Some(0);
        assert_eq!(
            RouteUpdate::from_patch_at(&p, 1_755_000_000),
            Some(RouteUpdate::clear())
        );
        p.status = Some(2);
        assert!(
            RouteUpdate::from_patch_at(&p, 1_755_000_000).is_none(),
            "a status-only active update has nothing to forward"
        );
    }

    #[test]
    fn eta_units_are_resolved_against_the_session() {
        let now = 1_755_000_000;
        let at = |v: f64| {
            RouteUpdate::from_patch_at(
                &NavigationPatch {
                    status: Some(1),
                    eta_epoch: Some(v),
                    ..Default::default()
                },
                now,
            )
            .and_then(|route| route.eta_epoch)
        };
        assert_eq!(at(1_755_100_000.0), Some(1_755_100_000));
        let apple_seconds = 1_755_100_000u64 - 978_307_200;
        assert_eq!(at(apple_seconds as f64), Some(1_755_100_000));
        assert_eq!(at((apple_seconds * 1000) as f64), Some(1_755_100_000));
        assert_eq!(at(1_600_000_000.0), None, "a different year is not an ETA");
        assert_eq!(at(f64::NAN), None);
        assert_eq!(at(1.5), None);
    }

    #[test]
    fn a_status_only_route_is_not_forwardable() {
        let p = patch("4040000b52010005000101");
        assert_eq!(p.status, Some(1));
        assert_eq!(RouteUpdate::from_patch_at(&p, 1_755_000_000), None);
    }

    #[test]
    fn invalid_numbers_and_text_stay_absent() {
        let mut p = NavigationPatch {
            remain_distance: Some(-1.0),
            distance_to_destination: Some(f64::NAN),
            road_name: Some("a\nb".into()),
            destination_name: Some("  ".into()),
            eta_epoch: Some(f64::INFINITY),
            time_to_destination: Some(-5.0),
            ..Default::default()
        };
        assert!(RouteUpdate::from_patch_at(&p, 1_755_000_000).is_none());
        p.status = Some(1);
        assert!(RouteUpdate::from_patch_at(&p, 1_755_000_000).is_none());
    }
}
