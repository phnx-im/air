// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/avatar/avatar.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card_tokens.dart';
import 'package:flutter/widgets.dart';

/// The actions a [ContactRequestCard] offers.
sealed class ContactRequestCardActions {
  const ContactRequestCardActions();
}

/// The recipient's answers to a request. A card with them keeps the sender's
/// picture and note covered until tapped.
final class ContactRequestAnswers extends ContactRequestCardActions {
  const ContactRequestAnswers({
    required this.acceptLabel,
    required this.dismissLabel,
    required this.onAccept,
    required this.onDismiss,
    this.isAccepting = false,
  });

  final String acceptLabel;

  /// Label for leaving the request unanswered.
  final String dismissLabel;

  final VoidCallback onAccept;
  final VoidCallback onDismiss;

  /// Whether accepting is in flight. Holds the accept button on a spinner and
  /// swallows a second tap.
  final bool isAccepting;
}

/// The sender's only action on their own request: taking it back.
final class ContactRequestRetraction extends ContactRequestCardActions {
  const ContactRequestRetraction({
    required this.label,
    required this.onRetract,
  });

  final String label;
  final VoidCallback onRetract;
}

/// The card that presents a contact request: a headline, the line naming how
/// the request travels, an optional framing line, the other person's avatar, an
/// optional note, and the actions that go with the reader's side of it.
///
/// A pure view: every string arrives from the host, and the only state it keeps
/// is what the reader has chosen to uncover. On a request someone else sent,
/// the picture and the note stay covered until tapped, so a stranger can't put
/// an image or a message in front of someone who hasn't agreed to see it. On
/// the reader's own request there is nothing to guard against, and nothing is
/// covered.
class ContactRequestCard extends StatefulWidget {
  const ContactRequestCard({
    super.key,
    required this.tokens,
    required this.title,
    this.subtitle,
    this.body,
    required this.displayName,
    this.gradientSeed,
    this.image,
    this.pictureRevealLabel,
    this.message,
    this.messageLabel,
    this.messageRevealLabel,
    required this.actions,
  }) : assert(
         actions is! ContactRequestAnswers ||
             image == null ||
             pictureRevealLabel != null,
         "a covered picture needs a prompt, or nothing invites the tap",
       ),
       assert(
         message == null ||
             (messageLabel != null &&
                 (actions is! ContactRequestAnswers ||
                     messageRevealLabel != null)),
         "a note needs a label, and a covered one a prompt",
       );

  final ContactRequestCardTokens tokens;

  /// The headline, naming what the card is.
  final String title;

  /// The line under the headline naming how the request travels: a username
  /// it went to, or a chat the two already share.
  final String? subtitle;

  /// A line framing the whole card, under the header and above the avatar.
  final String? body;

  /// Source of the avatar's fallback initial. The initial drops out while a
  /// picture is covered, so the circle gives nothing away.
  final String displayName;

  /// Seeds the avatar's fallback hue.
  final String? gradientSeed;

  /// The other person's picture. Decoding it is the host's business, so it
  /// arrives resolved.
  final ImageProvider? image;

  /// Prompt under the covered picture.
  final String? pictureRevealLabel;

  /// A note the sender attached.
  final String? message;

  /// Names the note as the sender's words rather than the card's own copy.
  final String? messageLabel;

  /// Prompt standing in for the covered note.
  final String? messageRevealLabel;

  final ContactRequestCardActions actions;

  @override
  State<ContactRequestCard> createState() => _ContactRequestCardState();
}

class _ContactRequestCardState extends State<ContactRequestCard> {
  late bool _pictureRevealed = widget.actions is! ContactRequestAnswers;
  late bool _messageRevealed = widget.actions is! ContactRequestAnswers;

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;
    final palette = SemanticPalette.of(context);
    final message = widget.message;
    final subtitle = widget.subtitle;
    final body = widget.body;

    return Padding(
      padding: tokens.containerPadding,
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(
            maxWidth: ContactRequestCardTokens.maxWidth,
          ),
          child: Container(
            width: double.infinity,
            padding: ContactRequestCardTokens.padding,
            decoration: BoxDecoration(
              color: palette.backgroundBase.secondary,
              borderRadius: BorderRadius.circular(
                ContactRequestCardTokens.radius,
              ),
            ),
            child: Column(
              mainAxisSize: .min,
              children: [
                Text(
                  widget.title,
                  textAlign: .center,
                  style: typeScale.header.regular.style(
                    color: palette.text.primary,
                    weight: Weight.emphasized,
                  ),
                ),
                if (subtitle != null)
                  ..._line(subtitle, palette.text.secondary),
                if (body != null) ..._line(body, palette.text.primary),
                Padding(
                  padding: ContactRequestCardTokens.avatarPadding,
                  child: _avatar(palette),
                ),
                if (message != null) _note(palette, message),
                const SizedBox(height: ContactRequestCardTokens.actionsTopGap),
                _actions(),
              ],
            ),
          ),
        ),
      ),
    );
  }

  List<Widget> _line(String text, Color color) => [
    const SizedBox(height: ContactRequestCardTokens.subtitleGap),
    Text(
      text,
      textAlign: .center,
      style: typeScale.body.regular.style(color: color),
    ),
  ];

  Widget _avatar(SemanticPalette palette) {
    final image = widget.image;

    // A prompt only stands while there's a picture left to uncover, so it
    // doubles as the signal that the circle is worth tapping.
    final prompt = image != null && !_pictureRevealed
        ? widget.pictureRevealLabel
        : null;

    return Column(
      mainAxisSize: .min,
      children: [
        Avatar(
          // A covered picture takes the initial with it: the circle should hold
          // nothing the reader hasn't asked to see.
          displayName: image != null ? "" : widget.displayName,
          size: ContactRequestCardTokens.avatarSize,
          image: _pictureRevealed ? image : null,
          gradientSeed: widget.gradientSeed,
          onTap: prompt != null
              ? () => setState(() => _pictureRevealed = true)
              : null,
        ),
        if (prompt != null) ...[
          const SizedBox(height: ContactRequestCardTokens.avatarLabelGap),
          Text(
            prompt,
            textAlign: .center,
            style: typeScale.body.xs.style(color: palette.text.tertiary),
          ),
        ],
      ],
    );
  }

  Widget _note(SemanticPalette palette, String message) {
    final label = widget.messageLabel;
    final prompt = _messageRevealed ? null : widget.messageRevealLabel;

    return MouseRegion(
      cursor: prompt != null ? SystemMouseCursors.click : MouseCursor.defer,
      child: GestureDetector(
        behavior: .opaque,
        onTap: prompt != null
            ? () => setState(() => _messageRevealed = true)
            : null,
        child: Column(
          mainAxisSize: .min,
          children: [
            if (label != null) ...[
              Text(
                label,
                textAlign: .center,
                style: typeScale.body.regular.style(
                  color: palette.text.primary,
                  weight: Weight.emphasized,
                ),
              ),
              const SizedBox(height: ContactRequestCardTokens.messageLabelGap),
            ],
            Text(
              prompt ?? message,
              textAlign: .center,
              style: prompt != null
                  ? typeScale.body.xs.style(color: palette.text.tertiary)
                  : typeScale.body.regular.style(color: palette.text.secondary),
            ),
          ],
        ),
      ),
    );
  }

  Widget _actions() => switch (widget.actions) {
    ContactRequestAnswers(
      :final acceptLabel,
      :final dismissLabel,
      :final onAccept,
      :final onDismiss,
      :final isAccepting,
    ) =>
      Row(
        children: [
          Expanded(
            child: Button(
              size: ButtonSize.large,
              type: ButtonType.secondary,
              onPressed: onDismiss,
              label: dismissLabel,
            ),
          ),
          const SizedBox(width: ContactRequestCardTokens.actionsGap),
          Expanded(
            child: Button(
              size: ButtonSize.large,
              type: ButtonType.primary,
              state: isAccepting ? ButtonState.pending : ButtonState.active,
              onPressed: onAccept,
              label: acceptLabel,
            ),
          ),
        ],
      ),
    ContactRequestRetraction(:final label, :final onRetract) => SizedBox(
      width: double.infinity,
      child: Button(
        size: ButtonSize.large,
        type: ButtonType.secondary,
        tone: ButtonTone.danger,
        onPressed: onRetract,
        label: label,
      ),
    ),
  };
}
