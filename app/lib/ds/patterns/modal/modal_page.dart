// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:flutter/material.dart';

/// A modal in the router's page stack, presented per surface: a card floating
/// over the two-pane layout where there's room beside it, an ordinary pushed
/// screen where there isn't.
class ModalPage<T> extends Page<T> {
  const ModalPage({
    super.key,
    super.name,
    super.arguments,
    required this.child,
  });

  final Widget child;

  @override
  Route<T> createRoute(BuildContext context) {
    // We resolve this here rather than at construction: createRoute runs under
    // the navigator, so the route matches the layout the modal opens into.
    if (ModalShellTokens.isFullBleed(context)) {
      return MaterialPageRoute<T>(settings: this, builder: (context) => child);
    }

    late final ModalCardRoute<T> route;
    return route = ModalCardRoute<T>(
      settings: this,
      builder: (context) => (route.settings as ModalPage<T>).child,
      barrierColor: SemanticPalette.of(context).function.neutral.scrim,
    );
  }
}
