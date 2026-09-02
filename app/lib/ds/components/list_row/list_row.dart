// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// One row of a list: an optional leading slot, a label with an optional
/// sublabel, and an optional trailing slot.
///
/// Two looks. With a [fill] the row paints a rounded tile and stands on its
/// own. Without one it paints a bottom hairline, so a run of rows reads as a
/// divided list. A row with an [onTap] routes hover / press / focus through the
/// shared [StateLayer], shaped by the platform -- touch dips, a pointer washes.
class ListRow extends StatelessWidget {
  const ListRow({
    super.key,
    required this.tokens,
    required this.label,
    this.leading,
    this.sublabel,
    this.trailing,
    this.fill,
    this.radius,
    this.separator = true,
    this.labelStyle,
    this.destructive = false,
    this.labelMaxLines = 1,
    this.onTap,
    this.enabled = true,
    this.hover,
  });

  final ListRowTokens tokens;
  final String label;

  final Widget? leading;
  final String? sublabel;
  final Widget? trailing;

  /// Tile fill. Set it for the tile look, leave it null for the separator look.
  final Color? fill;

  /// Corner radius of the tile. Defaults to [ListRowTokens.radius] and has no
  /// effect without a [fill].
  final double? radius;

  /// Whether the separator look paints its hairline. Off for the last row of a
  /// run, where the group's edge already closes the list.
  final bool separator;

  /// Label style override, for a row whose label carries its own weight or
  /// color. Defaults to body regular in `text.primary`.
  final TextStyle? labelStyle;

  /// A destructive row draws its label in the error role at rest. Like every
  /// row it takes the hover ink while hovered. Ignored when [labelStyle] is
  /// given.
  final bool destructive;

  /// Lines the label takes before it ellipsizes. Null lets it wrap as far as it
  /// needs, for a row carrying a paragraph rather than a name.
  final int? labelMaxLines;

  final VoidCallback? onTap;
  final bool enabled;

  /// Whether a pointer paints the hover wash. Follows the device by default,
  /// off for touch.
  final bool? hover;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final tileRadius = radius ?? ListRowTokens.radius;

    final decoration = switch (fill) {
      final Color color => BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(tileRadius),
      ),
      null when separator => BoxDecoration(
        border: Border(
          bottom: BorderSide(
            color: palette.separator.secondary,
            width: ListRowTokens.separatorWidth,
          ),
        ),
      ),
      null => null,
    };

    // Built under the state layer, so a hovered row hands the text its ink.
    final content = Builder(
      builder: (context) {
        final text = PanelSurface.textOf(context);
        final labelColor = destructive
            ? PanelSurface.inkOf(context) ?? palette.function.danger
            : text.primary;
        return Container(
          constraints: BoxConstraints(minHeight: tokens.height),
          padding: ListRowTokens.padding,
          child: Row(
            children: [
              if (leading != null) ...[
                leading!,
                const SizedBox(width: ListRowTokens.leadingGap),
              ],
              Expanded(
                child: Column(
                  mainAxisSize: .min,
                  crossAxisAlignment: .start,
                  children: [
                    Text(
                      label,
                      style:
                          labelStyle ??
                          typeScale.body.regular.style(
                            color: labelColor,
                            tight: true,
                          ),
                      maxLines: labelMaxLines,
                      overflow: labelMaxLines == null ? .clip : .ellipsis,
                    ),
                    if (sublabel != null) ...[
                      const SizedBox(height: ListRowTokens.sublabelGap),
                      Text(
                        sublabel!,
                        style: typeScale.body.s.style(
                          color: text.tertiary,
                          tight: true,
                        ),
                        maxLines: 1,
                        overflow: .ellipsis,
                      ),
                    ],
                  ],
                ),
              ),
              if (trailing != null) ...[
                const SizedBox(width: ListRowTokens.trailingGap),
                trailing!,
              ],
            ],
          ),
        );
      },
    );

    if (onTap == null) {
      return decoration == null
          ? content
          : DecoratedBox(decoration: decoration, child: content);
    }

    return StateLayer(
      // The wash follows the look: rounded where the row paints a tile, square
      // across the whole footprint where it only carries a hairline.
      borderRadius: fill != null ? tileRadius : CornerRadius.px0,
      surface: fill ?? palette.backgroundBase.primary,
      enabled: enabled,
      onTap: onTap,
      hover: hover,
      background: decoration == null
          ? null
          : DecoratedBox(decoration: decoration),
      child: content,
    );
  }
}
