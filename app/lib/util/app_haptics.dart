// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/services.dart';

/// Semantic haptics used across the app.
///
/// Call sites name the interaction, not the pattern, so the mapping from
/// interaction to platform pattern stays in one place.
///
/// iOS has a dedicated notification-warning haptic, Android does not, so
/// [destructive] uses a heavy impact as the portable baseline. Closer iOS
/// parity can later branch on `Platform.isIOS` inside [destructive] without
/// touching the public API.
///
/// [HapticFeedback] already honors the OS-level system haptics setting, so
/// there is no app-level gate.
///
/// The methods are fire-and-forget: they kick off the platform call and return
/// immediately. Call sites must not await them.
abstract final class AppHaptics {
  /// Discrete selection change, e.g. toggling a reaction chip or picking an
  /// emoji.
  static void selection() {
    HapticFeedback.selectionClick();
  }

  /// A lightweight action committing, e.g. a quick reaction being selected.
  static void confirm() {
    HapticFeedback.lightImpact();
  }

  /// A context menu or action overlay appearing, e.g. a long-press menu.
  ///
  /// Shares a medium impact with [gestureTrigger] for now; kept as a
  /// separate method so the two can be tuned independently later.
  static void menuOpen() {
    HapticFeedback.mediumImpact();
  }

  /// A gesture crossing its trigger threshold, e.g. swipe-to-reply arming.
  ///
  /// Shares a medium impact with [menuOpen] for now; kept as a separate
  /// method so the two can be tuned independently later.
  static void gestureTrigger() {
    HapticFeedback.mediumImpact();
  }

  /// A destructive action being committed, i.e. fired from the confirm
  /// handler, not when the dialog opens, e.g. deleting a message or a chat.
  static void destructive() {
    HapticFeedback.heavyImpact();
  }
}
