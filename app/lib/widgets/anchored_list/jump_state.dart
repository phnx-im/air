// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// Phases of the jump-to-message lifecycle.
///
/// - [idle]: no jump in progress.
/// - [loading]: the target ID isn't in the data yet; waiting for
///   `onLoadAround` to fetch it. The widget may show a loading indicator.
/// - [scrolling]: the target is in the data; the widget is actively
///   scrolling (or iteratively jumping) to bring it into view.
enum AnchoredListJumpPhase { idle, loading, scrolling }

/// Pure state machine for jump-to-message, decoupled from the widget so
/// it can be unit-tested independently.
///
/// Transitions: idle → loading → scrolling → idle
///                 ╰──── scrolling ───────╯
///                    (if already loaded)
///
/// A new [requestJump] from *any* phase replaces the current target,
/// effectively cancelling the previous jump.
class AnchoredListJumpState {
  AnchoredListJumpPhase phase = AnchoredListJumpPhase.idle;
  Object? targetId;

  /// Request a jump to [id].
  ///
  /// Returns the phase after the request.
  /// [isIdLoaded] checks whether the target ID exists in current data.
  /// [onLoadAround] is called when the ID is not loaded.
  AnchoredListJumpPhase requestJump(
    Object id, {
    required bool Function(Object id) isIdLoaded,
    required void Function(Object id) onLoadAround,
  }) {
    targetId = id;

    if (isIdLoaded(id)) {
      phase = AnchoredListJumpPhase.scrolling;
    } else {
      phase = AnchoredListJumpPhase.loading;
      onLoadAround(id);
    }
    return phase;
  }

  /// Called when data is updated. If we're loading and the target
  /// appears, transition to scrolling.
  ///
  /// Returns true if the phase changed to scrolling.
  bool onDataUpdated(bool Function(Object id) isIdLoaded) {
    if (phase != AnchoredListJumpPhase.loading || targetId == null) {
      return false;
    }
    if (isIdLoaded(targetId!)) {
      phase = AnchoredListJumpPhase.scrolling;
      return true;
    }
    return false;
  }

  void onScrollComplete() => _reset();

  void cancel() => _reset();

  void _reset() {
    phase = AnchoredListJumpPhase.idle;
    targetId = null;
  }
}
