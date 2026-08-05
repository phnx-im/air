// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/avatar/avatar.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card_tokens.dart';
import 'package:flutter/widgets.dart';

/// The card that presents an incoming contact request: a headline, the line
/// naming where the request came from, the sender's avatar, an optional note
/// they attached, and the pair of actions that answers it.
///
/// A pure view: every string arrives from the host, and the only state it keeps
/// is what the reader has chosen to uncover. The picture and the note stay
/// covered until tapped, so a stranger can't put an image or a message in
/// front of someone who hasn't agreed to see it.
class ContactRequestCard extends StatefulWidget {
  const ContactRequestCard({
    super.key,
    required this.tokens,
    required this.title,
    required this.subtitle,
    required this.displayName,
    this.gradientSeed,
    this.image,
    this.pictureRevealLabel,
    this.message,
    this.messageLabel,
    this.messageRevealLabel,
    required this.acceptLabel,
    required this.dismissLabel,
    required this.onAccept,
    required this.onDismiss,
    this.isAccepting = false,
  }) : assert(
         image == null || pictureRevealLabel != null,
         "a covered picture needs a prompt, or nothing invites the tap",
       ),
       assert(
         message == null ||
             (messageLabel != null && messageRevealLabel != null),
         "a covered note needs a label and a prompt, or nothing invites the tap",
       );

  final ContactRequestCardTokens tokens;

  /// The headline, naming what the card is.
  final String title;

  /// The line under the headline naming how the request reached the reader: a
  /// username they published, or a chat the two already share.
  final String subtitle;

  /// Source of the avatar's fallback initial. The initial drops out while a
  /// picture is covered, so the circle gives nothing away.
  final String displayName;

  /// Seeds the avatar's fallback hue.
  final String? gradientSeed;

  /// The sender's picture, covered until tapped. Decoding it is the host's
  /// business, so it arrives resolved.
  final ImageProvider? image;

  /// Prompt under the covered picture.
  final String? pictureRevealLabel;

  /// A note the sender attached, covered until tapped.
  final String? message;

  /// Names the note as the sender's words rather than the card's own copy.
  final String? messageLabel;

  /// Prompt standing in for the covered note.
  final String? messageRevealLabel;

  final String acceptLabel;

  /// Label for leaving the request unanswered.
  final String dismissLabel;

  final VoidCallback onAccept;
  final VoidCallback onDismiss;

  /// Whether accepting is in flight. Holds the accept button on a spinner and
  /// swallows a second tap.
  final bool isAccepting;

  @override
  State<ContactRequestCard> createState() => _ContactRequestCardState();
}

class _ContactRequestCardState extends State<ContactRequestCard> {
  bool _pictureRevealed = false;
  bool _messageRevealed = false;

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;
    final palette = SemanticPalette.of(context);
    final message = widget.message;

    return Padding(
      padding: tokens.containerPadding,
      child: Center(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: tokens.maxWidth),
          child: Container(
            width: double.infinity,
            padding: tokens.padding,
            decoration: BoxDecoration(
              color: palette.backgroundBase.secondary,
              borderRadius: BorderRadius.circular(tokens.radius),
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  widget.title,
                  textAlign: TextAlign.center,
                  style: typeScale.header.regular.style(
                    color: palette.text.primary,
                    weight: Weight.emphasized,
                  ),
                ),
                SizedBox(height: tokens.subtitleGap),
                Text(
                  widget.subtitle,
                  textAlign: TextAlign.center,
                  style: typeScale.body.regular.style(
                    color: palette.text.secondary,
                  ),
                ),
                Padding(padding: tokens.avatarPadding, child: _avatar(palette)),
                if (message != null) _note(palette, message),
                SizedBox(height: tokens.actionsTopGap),
                _actions(),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _avatar(SemanticPalette palette) {
    final tokens = widget.tokens;
    final image = widget.image;

    // A prompt only stands while there's a picture left to uncover, so it
    // doubles as the signal that the circle is worth tapping.
    final prompt = image != null && !_pictureRevealed
        ? widget.pictureRevealLabel
        : null;

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Avatar(
          // A covered picture takes the initial with it: the circle should hold
          // nothing the reader hasn't asked to see.
          displayName: image != null ? "" : widget.displayName,
          size: tokens.avatarSize,
          image: _pictureRevealed ? image : null,
          gradientSeed: widget.gradientSeed,
          onTap: prompt != null
              ? () => setState(() => _pictureRevealed = true)
              : null,
        ),
        if (prompt != null) ...[
          SizedBox(height: tokens.avatarLabelGap),
          Text(
            prompt,
            textAlign: TextAlign.center,
            style: typeScale.body.xs.style(color: palette.text.tertiary),
          ),
        ],
      ],
    );
  }

  Widget _note(SemanticPalette palette, String message) {
    final tokens = widget.tokens;
    final label = widget.messageLabel;
    final prompt = _messageRevealed ? null : widget.messageRevealLabel;

    return MouseRegion(
      cursor: prompt != null ? SystemMouseCursors.click : MouseCursor.defer,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: prompt != null
            ? () => setState(() => _messageRevealed = true)
            : null,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (label != null) ...[
              Text(
                label,
                textAlign: TextAlign.center,
                style: typeScale.body.regular.style(
                  color: palette.text.primary,
                  weight: Weight.emphasized,
                ),
              ),
              SizedBox(height: tokens.messageLabelGap),
            ],
            Text(
              prompt ?? message,
              textAlign: TextAlign.center,
              style: prompt != null
                  ? typeScale.body.xs.style(color: palette.text.tertiary)
                  : typeScale.body.regular.style(color: palette.text.secondary),
            ),
          ],
        ),
      ),
    );
  }

  Widget _actions() => Row(
    children: [
      Expanded(
        child: Button(
          size: ButtonSize.large,
          type: ButtonType.secondary,
          onPressed: widget.onDismiss,
          label: widget.dismissLabel,
        ),
      ),
      SizedBox(width: widget.tokens.actionsGap),
      Expanded(
        child: Button(
          size: ButtonSize.large,
          type: ButtonType.primary,
          state: widget.isAccepting ? ButtonState.pending : ButtonState.active,
          onPressed: widget.onAccept,
          label: widget.acceptLabel,
        ),
      ),
    ],
  );
}
