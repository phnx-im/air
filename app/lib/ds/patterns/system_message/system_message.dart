// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/system_message/system_message_tokens.dart';
import 'package:flutter/widgets.dart';

/// How much of the conversation a system message takes.
enum SystemMessageVariant {
  /// A line of centered text under a short rule: somebody joined, left, or
  /// renamed the chat. The shape most events take.
  notice,

  /// A centered card. For the few events that open or change a conversation
  /// outright and are worth stopping on.
  card,
}

/// Whether the event is routine or something that went wrong.
enum SystemMessageTone { neutral, danger }

/// An event in the conversation rather than a message in it.
///
/// A pure renderer. Every event has its own sentence, built from names and
/// titles the host resolves and localizes, so the text arrives finished: as a
/// [label] where it's one plain run, or as [content] where a name inside it
/// carries emphasis. Build those spans from [styleOf] and [emphasisOf] and
/// they'll match the ones the pattern paints itself.
class SystemMessage extends StatelessWidget {
  const SystemMessage({
    super.key,
    required this.tokens,
    this.variant = SystemMessageVariant.notice,
    this.tone = SystemMessageTone.neutral,
    this.label,
    this.content,
    this.title,
    this.timestamp,
  }) : assert(
         label == null || content == null,
         'pass the text once, as either label or content',
       ),
       assert(
         variant == SystemMessageVariant.card ||
             label != null ||
             content != null,
         'a notice is its text, so it needs one',
       );

  final SystemMessageTokens tokens;
  final SystemMessageVariant variant;
  final SystemMessageTone tone;

  /// The event, as one plain run of text.
  final String? label;

  /// The event, as spans, for a sentence that emphasizes the names in it.
  final InlineSpan? content;

  /// Heading of a [SystemMessageVariant.card], above its text.
  final String? title;

  /// When the event happened, rendered under it. The host's: a conversation's
  /// timestamps are relative, localized, and tick as they age.
  final Widget? timestamp;

  /// Base style of [variant]'s text. Spans passed as [content] inherit it, so
  /// only an emphasized run needs a style of its own.
  static TextStyle styleOf(
    BuildContext context,
    SystemMessageVariant variant,
  ) => _textStyle(context, variant, SystemMessageTone.neutral);

  /// Style for a name or a title inside the sentence.
  static TextStyle emphasisOf(
    BuildContext context,
    SystemMessageVariant variant,
  ) => _textStyle(
    context,
    variant,
    SystemMessageTone.neutral,
    weight: Weight.emphasized,
  );

  @override
  Widget build(BuildContext context) {
    final label = this.label;
    final span = content ?? (label == null ? null : TextSpan(text: label));

    return switch (variant) {
      SystemMessageVariant.notice => _Notice(
        tokens: tokens,
        tone: tone,
        span: span!,
        timestamp: timestamp,
      ),
      SystemMessageVariant.card => _Card(
        tokens: tokens,
        tone: tone,
        title: title,
        span: span,
        timestamp: timestamp,
      ),
    };
  }
}

class _Notice extends StatelessWidget {
  const _Notice({
    required this.tokens,
    required this.tone,
    required this.span,
    this.timestamp,
  });

  final SystemMessageTokens tokens;
  final SystemMessageTone tone;
  final InlineSpan span;
  final Widget? timestamp;

  @override
  Widget build(BuildContext context) {
    final timestamp = this.timestamp;

    return Padding(
      padding: tokens.padding,
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: tokens.ruleWidth,
              height: tokens.ruleThickness,
              color: SemanticPalette.of(context).separator.primary,
            ),
            SizedBox(height: tokens.ruleGap),
            Text.rich(
              span,
              textAlign: TextAlign.center,
              style: _textStyle(context, SystemMessageVariant.notice, tone),
            ),
            if (timestamp != null) ...[
              SizedBox(height: tokens.timestampGap),
              timestamp,
            ],
          ],
        ),
      ),
    );
  }
}

class _Card extends StatelessWidget {
  const _Card({
    required this.tokens,
    required this.tone,
    this.title,
    this.span,
    this.timestamp,
  });

  final SystemMessageTokens tokens;
  final SystemMessageTone tone;
  final String? title;
  final InlineSpan? span;
  final Widget? timestamp;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final title = this.title;
    final span = this.span;
    final timestamp = this.timestamp;

    return Padding(
      padding: tokens.padding,
      child: Center(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: tokens.cardMaxWidth),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: double.infinity,
                padding: tokens.cardPadding,
                decoration: BoxDecoration(
                  color: palette.backgroundBase.secondary,
                  borderRadius: BorderRadius.circular(tokens.cardRadius),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (title != null)
                      Text(
                        title,
                        textAlign: TextAlign.center,
                        style: typeScale.header.regular.style(
                          color: _tint(palette, tone) ?? palette.text.primary,
                          weight: Weight.emphasized,
                        ),
                      ),
                    if (title != null && span != null)
                      SizedBox(height: tokens.cardTitleGap),
                    if (span != null)
                      Text.rich(
                        span,
                        textAlign: TextAlign.center,
                        style: _textStyle(
                          context,
                          SystemMessageVariant.card,
                          tone,
                        ),
                      ),
                  ],
                ),
              ),
              if (timestamp != null) ...[
                SizedBox(height: tokens.cardTimestampGap),
                timestamp,
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// A notice sits at the conversation's smallest size, a card at its reading
/// size: the card is something to read, the notice something to skim past.
TextStyle _textStyle(
  BuildContext context,
  SystemMessageVariant variant,
  SystemMessageTone tone, {
  Weight weight = Weight.regular,
}) {
  final palette = SemanticPalette.of(context);
  final (token, color) = switch (variant) {
    SystemMessageVariant.notice => (typeScale.body.s, palette.text.tertiary),
    SystemMessageVariant.card => (
      typeScale.body.regular,
      palette.text.secondary,
    ),
  };
  return token.style(color: _tint(palette, tone) ?? color, weight: weight);
}

/// The colour a tone forces on the text, or null where it leaves it alone.
Color? _tint(SemanticPalette palette, SystemMessageTone tone) => switch (tone) {
  SystemMessageTone.neutral => null,
  SystemMessageTone.danger => palette.function.danger,
};
