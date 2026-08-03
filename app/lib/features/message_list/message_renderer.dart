// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:convert';
import 'dart:typed_data';

import 'package:air/core/api/markdown.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/patterns/message_text/message_text.dart';
import 'package:air/ds/patterns/message_text/message_text_tokens.dart';
import 'package:air/ds/patterns/reply_block/reply_block_tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/features/attachments/attachment_thumbnail.dart';
import 'package:air/features/emoji/jumbo_emoji.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:url_launcher/url_launcher.dart';

/// Who a quoted message is from and the line of it a reply shows.
///
/// A quote can fail to resolve for more than one reason -- the original was
/// deleted locally, or it predates the reader joining the group -- and the two
/// cannot be told apart, so both fall back to the same stand-in. A null sender
/// means there is nobody left to name.
({String? senderName, String preview}) quotedMessage(
  BuildContext context,
  UiInReplyToMessage inReplyTo,
) {
  final loc = AppLocalizations.of(context);
  return switch (inReplyTo) {
    UiInReplyToMessage_NotFound() => (
      senderName: loc.composer_reply_noaccess_message_user,
      preview: loc.composer_reply_noaccess_message_placeholder,
    ),
    UiInReplyToMessage_Resolved(:final sender, :final mimiContent)
        when !mimiContent.isDeleted =>
      (
        senderName: context.select(
          (UsersCubit cubit) => cubit.state.displayName(userId: sender),
        ),
        preview:
            mimiContent.plaintextPreview(loc) ??
            loc.composer_reply_noaccess_message_placeholder,
      ),
    UiInReplyToMessage_Resolved() => (
      senderName: null,
      preview: loc.composer_reply_deleted_message_placeholder,
    ),
  };
}

/// The still a quote shows for a message that is a picture, and null for one
/// that is not -- text, a file, or an original that never resolved.
Widget? quotedThumbnail(
  BuildContext context,
  UiInReplyToMessage inReplyTo,
  ReplyBlockTokens tokens,
) {
  final attachment = switch (inReplyTo) {
    UiInReplyToMessage_Resolved(:final mimiContent)
        when !mimiContent.isDeleted =>
      mimiContent.attachments.firstOrNull,
    _ => null,
  };
  if (attachment == null || attachment.imageMetadata == null) return null;
  return AttachmentThumbnail(
    attachment: attachment,
    size: tokens.thumbSize,
    radius: tokens.thumbRadius,
  );
}

/// Renders one block of a message body.
///
/// Every block reads its geometry from [tokens] and its ink from the message
/// palette, so a body stays one typographic system however deeply the markdown
/// nests. Text colour is inherited rather than set: the body style comes from
/// [MessageText], and a quote re-tints everything under it by overriding that
/// inherited style once.
Widget buildBlockElement(
  BuildContext context,
  BlockElement block,
  bool isSender,
  MessageTextTokens tokens,
) {
  return switch (block) {
    BlockElement_Paragraph(:final field0) => _paragraph(
      context,
      field0,
      isSender,
      tokens,
    ),
    BlockElement_Heading(:final field0) => _heading(
      context,
      field0,
      isSender,
      tokens,
    ),
    BlockElement_Quote(:final field0) => _quote(
      context,
      field0,
      isSender,
      tokens,
    ),
    BlockElement_UnorderedList(:final field0) => _list(
      context,
      field0,
      isSender,
      tokens,
      offset: null,
    ),
    BlockElement_OrderedList(field0: final offset, field1: final items) =>
      _list(context, items, isSender, tokens, offset: offset),
    BlockElement_Table(:final head, :final rows) => _table(
      context,
      head,
      rows,
      isSender,
      tokens,
    ),
    BlockElement_HorizontalRule() => _rule(context, isSender),
    BlockElement_CodeBlock(:final field0) => _codeBlock(
      context,
      field0,
      isSender,
      tokens,
    ),
    BlockElement_Error(:final field0) => _errorBlock(context, field0, tokens),
  };
}

Widget _paragraph(
  BuildContext context,
  List<RangedInlineElement> inlines,
  bool isSender,
  MessageTextTokens tokens,
) => Text.rich(
  TextSpan(
    children: inlines
        .map((child) => buildInlineElement(context, child, isSender, tokens))
        .toList(),
    // Size only. The colour is the one the body already carries, which is what
    // lets a quote re-tint the paragraphs nested inside it.
    style: isJumboEmoji(inlines) ? typeScale.emoji.jumbo.style() : null,
  ),
  softWrap: true,
  textWidthBasis: TextWidthBasis.longestLine,
);

Widget _heading(
  BuildContext context,
  List<RangedInlineElement> inlines,
  bool isSender,
  MessageTextTokens tokens,
) => Text.rich(
  TextSpan(
    children: inlines
        .map((child) => buildInlineElement(context, child, isSender, tokens))
        .toList(),
    style: typeScale.body.m.style(weight: Weight.emphasized),
  ),
);

/// A quoted passage: a rule down the leading edge and the quoted blocks beside
/// it, in the quieter ink a quote is set in. Nests, so a quote inside a quote
/// stacks another rule.
Widget _quote(
  BuildContext context,
  List<RangedBlockElement> blocks,
  bool isSender,
  MessageTextTokens tokens,
) {
  final palette = SemanticPalette.of(context);
  return Container(
    decoration: BoxDecoration(
      border: Border(
        left: BorderSide(
          color: palette.separator.primary,
          width: tokens.quoteBarWidth,
        ),
      ),
    ),
    padding: EdgeInsets.only(left: tokens.quoteGap),
    child: DefaultTextStyle.merge(
      style: TextStyle(color: palette.text.secondary),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        spacing: tokens.blockGap,
        children: [
          for (final inner in blocks)
            buildBlockElement(context, inner.element, isSender, tokens),
        ],
      ),
    ),
  );
}

/// A list of either kind. The marker column carries a bullet, a number, or --
/// where the item opens with a task marker -- a checkbox in the bullet's place,
/// so a checklist reads as one column of boxes rather than boxes behind dots.
///
/// [offset] is the number the list starts at, and null for an unordered list.
Widget _list(
  BuildContext context,
  List<List<RangedBlockElement>> items,
  bool isSender,
  MessageTextTokens tokens, {
  required BigInt? offset,
}) {
  // Sized to the list's last number, which is its longest.
  final markerWidth = offset == null || items.isEmpty
      ? null
      : _numberColumnWidth(
          context,
          offset + BigInt.from(items.length - 1),
          tokens,
        );

  return Padding(
    padding: EdgeInsets.symmetric(vertical: tokens.listPaddingY),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: tokens.listItemGap,
      children: [
        for (final (index, item) in items.indexed)
          _listItem(
            context,
            _TaskMarker.take(item),
            isSender,
            tokens,
            number: offset == null ? null : offset + BigInt.from(index),
            numberWidth: markerWidth,
          ),
      ],
    ),
  );
}

Widget _listItem(
  BuildContext context,
  _TaskMarker item,
  bool isSender,
  MessageTextTokens tokens, {
  required BigInt? number,
  required double? numberWidth,
}) {
  final palette = SemanticPalette.of(context);
  final ink = isSender
      ? palette.message.selfListPrefix
      : palette.message.otherListPrefix;

  // Every marker occupies one line's height, so it sits on the item's first
  // line rather than on the top of a block that may run several lines.
  final line = typeScale.body.regular.lineHeightPx;
  final Widget marker;
  if (item.checked != null) {
    marker = _Checkbox(
      checked: item.checked!,
      isSender: isSender,
      tokens: tokens,
      line: line,
    );
  } else if (number != null) {
    marker = SizedBox(
      width: numberWidth,
      height: line,
      child: Align(
        alignment: Alignment.centerLeft,
        child: Text('$number.', style: _numberStyle(ink)),
      ),
    );
  } else {
    marker = _Bullet(color: ink, size: tokens.bulletSize, line: line);
  }

  return Row(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      marker,
      SizedBox(width: tokens.listMarkerGap),
      Flexible(
        fit: FlexFit.loose,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          spacing: tokens.blockGap,
          children: [
            for (final block in item.blocks)
              buildBlockElement(context, block.element, isSender, tokens),
          ],
        ),
      ),
    ],
  );
}

TextStyle _numberStyle([Color? color]) => typeScale.body.regular
    .style(color: color)
    .copyWith(fontFeatures: const [FontFeature.tabularFigures()]);

/// Width of the column a numbered list's markers sit in: its own longest
/// number, so every item's text starts at the same place.
double _numberColumnWidth(
  BuildContext context,
  BigInt longest,
  MessageTextTokens tokens,
) {
  final painter = TextPainter(
    text: TextSpan(text: '$longest.', style: _numberStyle()),
    textDirection: Directionality.of(context),
  )..layout();
  final width = painter.width;
  painter.dispose();
  return width < tokens.listMarkerWidth ? tokens.listMarkerWidth : width;
}

Widget _table(
  BuildContext context,
  List<List<RangedBlockElement>> head,
  List<List<List<RangedBlockElement>>> rows,
  bool isSender,
  MessageTextTokens tokens,
) {
  final palette = SemanticPalette.of(context);
  Widget cell(List<RangedBlockElement> blocks) => Padding(
    padding: tokens.tableCellPadding,
    child: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: tokens.blockGap,
      children: [
        for (final block in blocks)
          buildBlockElement(context, block.element, isSender, tokens),
      ],
    ),
  );

  return Table(
    border: TableBorder.all(
      color: isSender
          ? palette.message.selfTableBorder
          : palette.message.otherTableBorder,
      width: tokens.tableBorderWidth,
      borderRadius: BorderRadius.circular(tokens.tableRadius),
    ),
    defaultColumnWidth: const IntrinsicColumnWidth(),
    children: [
      TableRow(
        children: [
          for (final blocks in head)
            DefaultTextStyle.merge(
              style: const TextStyle(fontWeight: FontWeight.bold),
              child: cell(blocks),
            ),
        ],
      ),
      for (final row in rows)
        TableRow(children: [for (final c in row) cell(c)]),
    ],
  );
}

Widget _rule(BuildContext context, bool isSender) {
  final message = SemanticPalette.of(context).message;
  return SizedBox(
    width: 100,
    child: Divider(color: isSender ? message.selfText : message.otherText),
  );
}

/// A fenced code block, on a slab of its own so it separates from the prose
/// around it without the bubble having to change shape.
Widget _codeBlock(
  BuildContext context,
  List<RangedCodeBlock> lines,
  bool isSender,
  MessageTextTokens tokens,
) {
  final palette = SemanticPalette.of(context);
  return Container(
    padding: tokens.codePadding,
    decoration: BoxDecoration(
      color: palette.fill.tertiary,
      borderRadius: BorderRadius.circular(tokens.codeRadius),
    ),
    child: Text.rich(
      TextSpan(
        text: lines.map((line) => line.value).join('\n'),
        style: typeScale.body.regular
            .style(
              color: isSender
                  ? palette.message.selfText
                  : palette.message.otherText,
            )
            .withSystemMonospace(),
      ),
    ),
  );
}

Widget _errorBlock(
  BuildContext context,
  String message,
  MessageTextTokens tokens,
) {
  final palette = SemanticPalette.of(context);
  return Container(
    padding: tokens.codePadding,
    decoration: BoxDecoration(
      border: Border(
        left: BorderSide(
          color: palette.separator.primary,
          width: tokens.quoteBarWidth,
        ),
      ),
      color: palette.function.warning.primary,
    ),
    child: Text.rich(TextSpan(text: message)),
  );
}

/// A list item split from the task marker it opens with, if it opens with one.
///
/// The parser hands a task item back as an ordinary list item whose first
/// paragraph begins with the marker. Lifting the marker out here is what lets
/// the checkbox take the bullet's place in the column instead of appearing
/// behind it, mid-sentence.
class _TaskMarker {
  const _TaskMarker(this.checked, this.blocks);

  /// Null where the item is not a task item at all.
  final bool? checked;
  final List<RangedBlockElement> blocks;

  static _TaskMarker take(List<RangedBlockElement> blocks) {
    final first = blocks.firstOrNull?.element;
    if (first is! BlockElement_Paragraph) return _TaskMarker(null, blocks);
    final marker = first.field0.firstOrNull;
    if (marker?.element case InlineElement_TaskListMarker(:final field0)) {
      final head = blocks.first;
      return _TaskMarker(field0, [
        RangedBlockElement(
          start: head.start,
          end: head.end,
          element: BlockElement.paragraph(first.field0.sublist(1)),
        ),
        ...blocks.skip(1),
      ]);
    }
    return _TaskMarker(null, blocks);
  }
}

class _Bullet extends StatelessWidget {
  const _Bullet({required this.color, required this.size, required this.line});

  final Color color;
  final double size;

  /// Height of one line of the item's text, which the dot centres on.
  final double line;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: size,
    height: line,
    child: Center(
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(shape: BoxShape.circle, color: color),
      ),
    ),
  );
}

class _Checkbox extends StatelessWidget {
  const _Checkbox({
    required this.checked,
    required this.isSender,
    required this.tokens,
    required this.line,
  });

  final bool checked;
  final bool isSender;
  final MessageTextTokens tokens;

  /// Height of one line of the item's text, which the box centres on.
  final double line;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final message = palette.message;
    return SizedBox(
      width: tokens.checkboxSize,
      height: line,
      child: Center(
        child: Container(
          width: tokens.checkboxSize,
          height: tokens.checkboxSize,
          decoration: BoxDecoration(
            // The page background, not a message fill: the box has to read
            // against either bubble, and a tint of one of them does not.
            color: palette.backgroundBase.primary,
            borderRadius: BorderRadius.circular(tokens.checkboxRadius),
          ),
          child: checked
              ? CustomPaint(
                  painter: _CheckPainter(
                    color: isSender
                        ? message.selfCheckboxCheck
                        : message.otherCheckboxCheck,
                    strokeWidth: tokens.checkmarkStrokeWidth,
                  ),
                )
              : null,
        ),
      ),
    );
  }
}

class _CheckPainter extends CustomPainter {
  const _CheckPainter({required this.color, required this.strokeWidth});

  final Color color;
  final double strokeWidth;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    // Drawn on a 12-unit grid and scaled to whatever box the token asks for.
    final scale = size.width / 12;
    final path = Path()
      ..moveTo(2.5 * scale, 6.5 * scale)
      ..lineTo(5 * scale, 9 * scale)
      ..lineTo(9.5 * scale, 3.5 * scale);
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_CheckPainter old) =>
      old.color != color || old.strokeWidth != strokeWidth;
}

InlineSpan buildInlineElement(
  BuildContext context,
  RangedInlineElement inline,
  bool isSender,
  MessageTextTokens tokens, {
  Uri? destUrl,
}) {
  final palette = SemanticPalette.of(context);
  return switch (inline.element) {
    InlineElement_Text(:final field0) => TextSpan(
      text: field0,
      recognizer: destUrl != null
          ? openLinkRecognizer(context, destUrl, field0)
          : null,
      mouseCursor: destUrl != null
          ? SystemMouseCursors.click
          : SystemMouseCursors.text,
    ),
    InlineElement_Code(:final field0) => TextSpan(
      text: field0,
      style: typeScale.body.xs.style().withSystemMonospace(),
    ),
    InlineElement_Link(:final destUrl, :final children) => TextSpan(
      children: children
          .map(
            (child) => buildInlineElement(
              context,
              child,
              isSender,
              tokens,
              destUrl: _parseLinkDest(destUrl),
            ),
          )
          .toList(),
      style: TextStyle(
        color: palette.function.link,
        decorationColor: palette.function.link,
        decoration: TextDecoration.underline,
      ),
    ),
    InlineElement_Bold(:final field0) => TextSpan(
      children: field0
          .map((child) => buildInlineElement(context, child, isSender, tokens))
          .toList(),
      style: const TextStyle(fontWeight: FontWeight.bold),
      recognizer: destUrl != null ? openLinkRecognizer(context, destUrl) : null,
      mouseCursor: destUrl != null
          ? SystemMouseCursors.click
          : SystemMouseCursors.text,
    ),
    InlineElement_Italic(:final field0) => TextSpan(
      children: field0
          .map((child) => buildInlineElement(context, child, isSender, tokens))
          .toList(),
      style: const TextStyle(fontStyle: FontStyle.italic),
      recognizer: destUrl != null ? openLinkRecognizer(context, destUrl) : null,
      mouseCursor: destUrl != null
          ? SystemMouseCursors.click
          : SystemMouseCursors.text,
    ),
    InlineElement_Strikethrough(:final field0) => TextSpan(
      children: field0
          .map((child) => buildInlineElement(context, child, isSender, tokens))
          .toList(),
      style: const TextStyle(decoration: TextDecoration.lineThrough),
      recognizer: destUrl != null ? openLinkRecognizer(context, destUrl) : null,
      mouseCursor: destUrl != null
          ? SystemMouseCursors.click
          : SystemMouseCursors.text,
    ),
    InlineElement_Spoiler(:final field0) => TextSpan(
      children: field0
          .map((child) => buildInlineElement(context, child, isSender, tokens))
          .toList(),
      style: TextStyle(
        decoration: TextDecoration.combine([
          TextDecoration.overline,
          TextDecoration.lineThrough,
          TextDecoration.underline,
        ]),
      ),
    ),
    InlineElement_Image() => const WidgetSpan(child: AppIcon.image()),
    // A task marker the parser did not put at the head of a list item, where
    // it would have been lifted into the marker column instead.
    InlineElement_TaskListMarker(:final field0) => WidgetSpan(
      alignment: PlaceholderAlignment.middle,
      child: Padding(
        padding: EdgeInsets.only(right: tokens.listMarkerGap),
        child: _Checkbox(
          checked: field0,
          isSender: isSender,
          tokens: tokens,
          line: tokens.checkboxSize,
        ),
      ),
    ),
  };
}

Uri? _parseLinkDest(String dest) {
  final uri = Uri.tryParse(dest);
  if (uri == null) return null;
  if (uri.hasScheme) return uri;
  // If the link doesn't have a scheme, try parsing it as https.
  return Uri.tryParse('https://$dest');
}

TapGestureRecognizer openLinkRecognizer(
  BuildContext context,
  Uri uri, [
  String? text,
]) => TapGestureRecognizer()
  ..onTap = () async {
    if (text == null || text != uri.toString()) {
      final shouldOpen = await _showLinkConfirmationDialog(context, uri);
      if (!shouldOpen) {
        return;
      }
    }

    if (await canLaunchUrl(uri)) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  };

Future<bool> _showLinkConfirmationDialog(BuildContext context, Uri uri) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (dialogContext) {
      final loc = AppLocalizations.of(context);
      final palette = SemanticPalette.of(dialogContext);

      return AppDialog(
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .center,
          children: [
            Center(
              child: Column(
                spacing: S.s8,
                children: [
                  Text(
                    loc.linkConfirmation_title,
                    style: typeScale.header.regular.style(
                      weight: Weight.emphasized,
                    ),
                  ),
                  Text(
                    loc.linkConfirmation_description,
                    textAlign: .center,
                    style: typeScale.body.regular.style(
                      color: palette.text.secondary,
                    ),
                  ),
                  Text(
                    uri.toString(),
                    textAlign: .center,
                    style: typeScale.body.xs.style(color: palette.text.primary),
                  ),
                ],
              ),
            ),

            const SizedBox(height: S.s24),
            Row(
              children: [
                Expanded(
                  child: Button(
                    onPressed: () => Navigator.of(dialogContext).pop(false),
                    label: loc.linkConfirmation_cancel,
                    type: ButtonType.secondary,
                  ),
                ),
                const SizedBox(width: S.s12),
                Expanded(
                  child: Button(
                    onPressed: () => Navigator.of(dialogContext).pop(true),
                    label: loc.linkConfirmation_openLink,
                  ),
                ),
              ],
            ),
          ],
        ),
      );
    },
  );

  return result ?? false;
}

// The style used for formatting characters like * or >
TextStyle highlightStyle(BuildContext context) =>
    TextStyle(color: SemanticPalette.of(context).function.link);

class CustomTextEditingController extends TextEditingController {
  // Keep track of where widgets are, so the cursor can treat it as one unit
  List<({int start, int end})> widgetRanges = [];
  int lastKnownRawTextLength = 0;
  int previousCursorPosition = 0;
  Uint8List raw = Uint8List(0);

  // Cache for buildTextSpan to avoid re-parsing on selection-only changes
  String? _cachedText;
  TextSpan? _cachedTextSpan;

  CustomTextEditingController() {
    addListener(_handleCursorMovement);
  }

  void _handleCursorMovement() {
    int cursorPosition = selection.extentOffset;

    if (cursorPosition == -1) {
      return;
    }

    if (lastKnownRawTextLength < text.length) {
      // Do nothing when writing text
      previousCursorPosition = cursorPosition;
      return;
    }

    // Convert position into UTF-8 index
    String charsUpToCursor = text.substring(0, cursorPosition);
    int cursorPositionUtf8 = utf8.encode(charsUpToCursor).length;

    if (lastKnownRawTextLength > text.length) {
      // Was part of a widget deleted? Then either:
      // - The user pressed backspace, so the cursor is now at the end of where the widget was
      // - The user pressed delete, so the cursor is still at the character just before where the widget was

      for (var range in widgetRanges) {
        if (cursorPosition >= range.start && cursorPosition < range.end) {
          int startUtf16 = utf8.decode(raw.sublist(0, range.start)).length;

          if (cursorPosition != previousCursorPosition) {
            // The cursor moved, so this was a backspace and not a delete
            var newText = text.replaceRange(startUtf16, cursorPosition, "");

            // Make sure we don't use outdated data
            widgetRanges.clear();
            lastKnownRawTextLength = newText.length;

            text = newText;

            moveCursorTo(startUtf16);
          } else {
            // The cursor did not move, this was a delete, not a backspace
            int endUtf16 = utf8.decode(raw.sublist(0, range.end)).length;
            var removedChars = lastKnownRawTextLength - text.length;
            var newText = text.replaceRange(
              cursorPosition,
              endUtf16 - removedChars,
              "",
            );

            // Make sure we don't use outdated data
            widgetRanges.clear();
            lastKnownRawTextLength = newText.length;

            text = newText;

            moveCursorTo(startUtf16);
          }

          break;
        }
      }

      previousCursorPosition = cursorPosition;
      return;
    }

    for (var range in widgetRanges) {
      // If the cursor is inside a widget range, push it to the edge
      if (cursorPositionUtf8 > range.start && cursorPositionUtf8 < range.end) {
        if (cursorPosition < previousCursorPosition) {
          int startUtf16 = utf8.decode(raw.sublist(0, range.start)).length;
          moveCursorTo(startUtf16);
        } else {
          int endUtf16 = utf8.decode(raw.sublist(0, range.end)).length;
          moveCursorTo(endUtf16);
        }

        break;
      }
    }
    previousCursorPosition = cursorPosition;
  }

  /// Move cursor/extent to [newPosition], avoiding re-entrant listener calls
  /// by temporarily removing the listener before setting the selection.
  void moveCursorTo(int newPosition) {
    removeListener(_handleCursorMovement);
    previousCursorPosition = newPosition;
    if (selection.baseOffset == selection.extentOffset) {
      selection = TextSelection(
        extentOffset: newPosition,
        baseOffset: newPosition,
        affinity: selection.affinity,
        isDirectional: selection.isDirectional,
      );
    } else {
      selection = TextSelection(
        extentOffset: newPosition,
        baseOffset: selection.baseOffset,
        affinity: selection.affinity,
        isDirectional: selection.isDirectional,
      );
    }
    addListener(_handleCursorMovement);
  }

  @override
  TextSpan buildTextSpan({
    required context,
    TextStyle? style,
    required bool withComposing,
  }) {
    // Return cached span when text hasn't changed (e.g. selection-only updates)
    if (text == _cachedText && _cachedTextSpan != null) {
      return TextSpan(style: style, children: _cachedTextSpan!.children);
    }

    // Regenerating this data
    widgetRanges.clear();
    lastKnownRawTextLength = text.length;

    // Flutter uses UTF-16, but Rust uses UTF-8
    raw = utf8.encode(text);

    MessageContent parsed = const MessageContent(elements: []);

    if (text.isNotEmpty) {
      parsed = MessageContent.parseMarkdownRaw(string: raw);
    }

    _cachedText = text;
    _cachedTextSpan = TextSpan(
      style: style,
      children: buildWrappedBlock(context, 0, raw.length, parsed.elements),
    );

    return TextSpan(style: style, children: _cachedTextSpan!.children);
  }

  InlineSpan buildFormattedTextSpanBlock(
    BuildContext context,
    RangedBlockElement block,
  ) {
    return switch (block.element) {
      BlockElement_Paragraph(:final field0) => TextSpan(
        children: buildWrappedInline(context, block.start, block.end, field0),
      ),
      BlockElement_Heading(:final field0) => TextSpan(
        children: buildWrappedInline(context, block.start, block.end, field0),
        style: const TextStyle(fontSize: 20),
      ),
      BlockElement_Quote(:final field0) => TextSpan(
        children: buildWrappedBlock(context, block.start, block.end, field0),
        style: TextStyle(color: Primitive.neutral(NeutralShade.s600)),
      ),
      BlockElement_UnorderedList(:final field0) => TextSpan(
        children: buildWrappedBlock(
          context,
          block.start,
          block.end,
          field0.expand((list) => list).toList(),
        ),
      ),
      BlockElement_OrderedList(:final field1) => TextSpan(
        children: buildWrappedBlock(
          context,
          block.start,
          block.end,
          field1.expand((list) => list).toList(),
        ),
      ),
      BlockElement_Table() => TextSpan(
        text: utf8.decode(raw.sublist(block.start, block.end)),
        style: highlightStyle(context),
      ),
      BlockElement_HorizontalRule() => TextSpan(
        text: utf8.decode(raw.sublist(block.start, block.end)),
        style: highlightStyle(context),
      ),
      BlockElement_CodeBlock(:final field0) => TextSpan(
        children: buildWrappedInline(
          context,
          block.start,
          block.end,
          field0
              .map(
                (item) => RangedInlineElement(
                  start: item.start,
                  end: item.end,
                  element: InlineElement.code(item.value),
                ),
              )
              .toList(),
        ),
        style: typeScale.body.xs.style().withSystemMonospace(),
      ),
      BlockElement_Error() => TextSpan(
        text: utf8.decode(raw.sublist(block.start, block.end)),
        style: TextStyle(
          color: SemanticPalette.of(context).function.danger,
          decorationColor: SemanticPalette.of(context).function.danger,
          decoration: TextDecoration.underline,
          decorationStyle: TextDecorationStyle.wavy,
        ),
      ),
    };
  }

  InlineSpan buildFormattedTextSpanInline(
    BuildContext context,
    RangedInlineElement inline,
  ) {
    return switch (inline.element) {
      // TODO: Handle this case.
      InlineElement_Text() => TextSpan(
        text: utf8.decode(raw.sublist(inline.start, inline.end)),
      ),
      InlineElement_Code() => TextSpan(
        text: utf8.decode(raw.sublist(inline.start, inline.end)),
        style: typeScale.body.xs.style().withSystemMonospace(),
      ),
      InlineElement_Link() => TextSpan(
        text: utf8.decode(raw.sublist(inline.start, inline.end)),
        style: TextStyle(
          color: SemanticPalette.of(context).function.link,
          decorationColor: SemanticPalette.of(context).function.link,
          decoration: TextDecoration.underline,
        ),
      ),
      InlineElement_Bold(:final field0) => TextSpan(
        children: buildWrappedInline(context, inline.start, inline.end, field0),
        style: const TextStyle(fontWeight: FontWeight.bold),
      ),
      InlineElement_Italic(:final field0) => TextSpan(
        children: buildWrappedInline(context, inline.start, inline.end, field0),
        style: const TextStyle(fontStyle: FontStyle.italic),
      ),
      InlineElement_Strikethrough(:final field0) => TextSpan(
        children: buildWrappedInline(context, inline.start, inline.end, field0),
        style: const TextStyle(decoration: TextDecoration.lineThrough),
      ),
      InlineElement_Spoiler(:final field0) => TextSpan(
        children: buildWrappedInline(context, inline.start, inline.end, field0),
        style: TextStyle(
          decoration: TextDecoration.combine([
            TextDecoration.overline,
            TextDecoration.lineThrough,
            TextDecoration.underline,
          ]),
        ),
      ),
      InlineElement_Image() => buildCorrectWidget(
        const AppIcon.image(size: 32),
        inline.start,
        inline.end,
      ),
      InlineElement_TaskListMarker() => TextSpan(
        text: utf8.decode(raw.sublist(inline.start, inline.end)),
        style: highlightStyle(context),
      ),
    };
  }

  InlineSpan buildCorrectWidget(Widget widget, int rangeStart, int rangeEnd) {
    widgetRanges.add((start: rangeStart, end: rangeEnd));

    return TextSpan(
      children: [
        WidgetSpan(child: widget),
        TextSpan(text: "\u200d" * (rangeEnd - rangeStart - 1)),
      ],
    );
  }

  List<InlineSpan> buildWrappedInline(
    BuildContext context,
    int rangeStart,
    int rangeEnd,
    List<RangedInlineElement> value,
  ) {
    List<InlineSpan> children = [];

    var lastInner = (start: 0, end: rangeStart);

    for (var inner in value) {
      if (inner.start < rangeStart) {
        // This element is outside of the surrounding block. Ignore.
        // This can happen for this markdown: "- [ ] > test"
        continue;
      }
      // Gap between previous and this inline
      if (lastInner.end < inner.start) {
        children.add(
          TextSpan(
            text: utf8.decode(raw.sublist(lastInner.end, inner.start)),
            style: highlightStyle(context),
          ),
        );
      }

      children.add(buildFormattedTextSpanInline(context, inner));
      lastInner = (start: inner.start, end: inner.end);
    }

    // Gap after last inline
    if (lastInner.end < rangeEnd) {
      children.add(
        TextSpan(
          text: utf8.decode(raw.sublist(lastInner.end, rangeEnd)),
          style: highlightStyle(context),
        ),
      );
    }

    return children;
  }

  List<InlineSpan> buildWrappedBlock(
    BuildContext context,
    int rangeStart,
    int rangeEnd,
    List<RangedBlockElement> value,
  ) {
    List<InlineSpan> children = [];

    var lastInner = (start: 0, end: rangeStart);

    for (var inner in value) {
      // Gap between previous and this block
      if (lastInner.end < inner.start) {
        children.add(
          TextSpan(
            text: utf8.decode(raw.sublist(lastInner.end, inner.start)),
            style: highlightStyle(context),
          ),
        );
      }

      children.add(buildFormattedTextSpanBlock(context, inner));

      lastInner = (start: inner.start, end: inner.end);
    }

    // Gap after last block
    if (lastInner.end < rangeEnd) {
      children.add(
        TextSpan(
          text: utf8.decode(raw.sublist(lastInner.end, rangeEnd)),
          style: highlightStyle(context),
        ),
      );
    }

    return children;
  }
}
