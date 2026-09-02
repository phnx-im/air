// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// Shared widgets of the developer surfaces: captioned card, copy-on-tap info
/// row, danger row.
///
/// We don't localize strings on these surfaces, they are read while debugging
/// a build.
library;

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/list_group/list_group.dart';
import 'package:air/ds/components/list_group/list_group_tokens.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/features/you/you_fields.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart' show SnackBar, Tooltip, showDialog;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Glyph size for a row's trailing icon on these surfaces.
const double developerRowIconSize = S.s16;

/// Tap target of a row's icon button. It overhangs the glyph slot, so the
/// glyph stays aligned with the plain row icons.
const double _rowButtonHitSize = S.s40;

/// An icon button that takes up no more of a row than a plain row icon does.
class DeveloperRowButton extends StatelessWidget {
  const DeveloperRowButton({
    super.key,
    required this.icon,
    required this.tooltip,
    required this.onPressed,
  });

  final AppIconType icon;
  final String tooltip;

  /// Null disables the button.
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: developerRowIconSize,
      child: OverflowBox(
        maxWidth: _rowButtonHitSize,
        maxHeight: _rowButtonHitSize,
        child: Tooltip(
          message: tooltip,
          child: ButtonIcon(
            variant: ButtonIconVariant.plain,
            size: ButtonIconSize.s32,
            iconSize: developerRowIconSize,
            hitTargetSize: _rowButtonHitSize,
            icon: icon,
            onPressed: onPressed,
          ),
        ),
      ),
    );
  }
}

/// Fill of the enclosing card, so a row washes against what it is painted on
/// rather than against the page below.
class _CardSurface extends InheritedWidget {
  const _CardSurface({required this.color, required super.child});

  final Color color;

  static Color? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_CardSurface>()?.color;

  @override
  bool updateShouldNotify(_CardSurface oldWidget) => oldWidget.color != color;
}

/// Fill for a developer card, and for whatever a row paints over it.
///
/// We take the profile pane fill rather than the grouped-card default, which
/// the desktop pane's surface swallows in dark.
Color developerCardFill(BuildContext context) =>
    _CardSurface.maybeOf(context) ?? youModuleFill(context);

/// A run of rows under an optional caption.
///
/// The card draws the hairlines between its rows, so [ListRow] children pass
/// `separator: false`.
class DeveloperCard extends StatelessWidget {
  const DeveloperCard({
    super.key,
    this.caption,
    this.fill,
    required this.children,
  });

  /// Names what the rows have in common. The first card of a section needs
  /// none, the host already draws the section title.
  final String? caption;

  /// Fill override, for a card that stands out from its siblings.
  final Color? fill;

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final color = fill ?? developerCardFill(context);

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (caption case final caption?) DeveloperCaption(caption),
        _CardSurface(
          color: color,
          child: ListGroup(
            tokens: ListGroupTokens.current,
            color: color,
            children: [
              for (int i = 0; i < children.length; i++) ...[
                if (i > 0) const _CardDivider(),
                children[i],
              ],
            ],
          ),
        ),
      ],
    );
  }
}

/// The caption above a card.
class DeveloperCaption extends StatelessWidget {
  const DeveloperCaption(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s8),
      child: Text(
        text.toUpperCase(),
        style: typeScale.body.xs.style(
          weight: Weight.emphasized,
          color: SemanticPalette.of(context).accentBrand.secondary,
        ),
      ),
    );
  }
}

/// A label and the value it names, copied to the clipboard on tap.
///
/// Not a [ListRow]: its sublabel is single-line, tertiary and never monospace,
/// where here the value is what is being read.
class DeveloperInfoRow extends StatelessWidget {
  const DeveloperInfoRow({
    super.key,
    required this.label,
    required this.value,
    this.monospace = false,
    this.content,
    this.trailing,
  });

  final String label;

  /// What the row copies, and what it shows unless [content] renders it.
  final String value;

  final bool monospace;

  /// Renders the value in place of the plain text. The clipboard still gets
  /// [value].
  final Widget? content;

  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    var valueStyle = typeScale.body.s.style(color: palette.text.primary);
    if (monospace) {
      valueStyle = valueStyle.withSystemMonospace();
    }

    return StateLayer(
      // Square and clipped by the card, like an unfilled [ListRow].
      borderRadius: CornerRadius.px0,
      surface: developerCardFill(context),
      onTap: () {
        Clipboard.setData(ClipboardData(text: value));
        showSnackBarStandalone(
          (loc) => SnackBar(
            content: Text('Copied $label'),
            duration: const Duration(seconds: 2),
          ),
        );
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
        child: Row(
          crossAxisAlignment: .start,
          children: [
            // Shares rather than a fixed label column: a column wide enough
            // for the desktop pane wraps a user ID every few characters on a
            // phone.
            Expanded(
              flex: 2,
              child: Text(
                label,
                style: typeScale.body.s.style(color: palette.text.tertiary),
              ),
            ),
            const SizedBox(width: S.s12),
            Expanded(
              flex: 3,
              child: Row(
                crossAxisAlignment: .start,
                children: [
                  Expanded(child: content ?? Text(value, style: valueStyle)),
                  if (trailing case final trailing?) ...[
                    const SizedBox(width: S.s8),
                    trailing,
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A danger zone row: danger-toned, and confirmed before it runs.
class DeveloperDangerRow extends StatelessWidget {
  const DeveloperDangerRow({
    super.key,
    required this.label,
    required this.icon,
    required this.confirmMessage,
    required this.confirmLabel,
    required this.onConfirm,
    this.sublabel,
  });

  final String label;
  final String? sublabel;
  final AppIconType icon;

  final String confirmMessage;
  final String confirmLabel;
  final VoidCallback onConfirm;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return ListRow(
      tokens: ListRowTokens.current,
      label: label,
      sublabel: sublabel,
      destructive: true,
      separator: false,
      // Built under the row's state layer, so a hovered row hands the icon
      // its ink.
      trailing: Builder(
        builder: (context) => AppIcon(
          type: icon,
          size: developerRowIconSize,
          color: PanelSurface.inkOf(context) ?? palette.function.danger,
        ),
      ),
      onTap: () => showDialog(
        context: context,
        builder: (_) => ConfirmDialog(
          title: 'Confirmation',
          message: confirmMessage,
          cancel: 'Cancel',
          confirm: confirmLabel,
          onConfirm: onConfirm,
          destructive: true,
        ),
      ),
    );
  }
}

class _CardDivider extends StatelessWidget {
  const _CardDivider();

  @override
  Widget build(BuildContext context) {
    return Container(
      height: StrokeWidth.px0_5,
      color: SemanticPalette.of(context).separator.secondary,
    );
  }
}
