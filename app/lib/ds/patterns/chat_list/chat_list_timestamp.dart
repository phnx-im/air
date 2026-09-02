// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// The "last activity" stamp beside a chat-list row's title.
///
/// A pure view. Which tier a moment falls into -- a minute count, a time, a
/// weekday, a date -- is a locale decision on a clock that keeps ticking, so
/// the host formats the label and owns the timer that refreshes it.
class ChatListTimestamp extends StatelessWidget {
  const ChatListTimestamp({super.key, required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Text(
      label,
      maxLines: 1,
      softWrap: false,
      // Tabular figures hold the stamp's width steady, so a minute count
      // ticking up never reflows the title beside it.
      style: typeScale.body.mini
          .style(color: PanelSurface.textOf(context).tertiary)
          .copyWith(
            height: 1.0,
            fontFeatures: const [FontFeature.tabularFigures()],
          ),
    );
  }
}
