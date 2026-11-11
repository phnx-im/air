// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/components/panel/panel_surface.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble_tokens.dart';
import 'package:air/ds/patterns/message_text/message_text.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/message_list/message_renderer.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/util/time/time_labels.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';

final _log = Logger('MessageDetails');

/// Opens the details of a message: the message as the chat shows it, and its
/// edit history once it was edited.
///
/// [preview] is built inside the modal, which sits on the root navigator above
/// the chat's providers. It can only read what is provided above that.
void showMessageDetails(
  BuildContext context, {
  required MessageId messageId,
  required Widget preview,
}) {
  final versions = context.read<ChatDetailsCubit>().messageVersions(
    messageId: messageId,
  );
  unawaited(
    versions.then(
      (_) {},
      onError: (Object error, StackTrace stackTrace) =>
          _log.severe('Failed to load message versions', error, stackTrace),
    ),
  );
  if (ModalShellTokens.isFullBleed(context)) {
    unawaited(
      showAdaptiveModal<void>(
        context: context,
        contentPadding: EdgeInsets.zero,
        builder: (_) =>
            MessageDetailsSheet(preview: preview, versions: versions),
      ),
    );
  } else {
    unawaited(
      showAppModal<void>(
        context: context,
        builder: (_) =>
            MessageDetailsModal(preview: preview, versions: versions),
      ),
    );
  }
}

/// The message details on a phone: a sheet with its own header.
class MessageDetailsSheet extends StatelessWidget {
  const MessageDetailsSheet({
    super.key,
    required this.preview,
    required this.versions,
  });

  final Widget preview;
  final Future<List<UiMessageVersion>> versions;

  @override
  Widget build(BuildContext context) {
    final surface = SemanticPalette.of(context).backgroundElevated.primary;
    return PanelSurface(
      color: surface,
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          DialogHeader(
            tokens: DialogHeaderTokens.of(context),
            title: AppLocalizations.of(context).messageDetails_title,
            onTrailing: () => Navigator.of(context).pop(),
            fill: surface,
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(S.s16, S.s16, S.s16, S.s24),
            child: MessageDetailsView(preview: preview, versions: versions),
          ),
        ],
      ),
    );
  }
}

/// The message details on a larger screen: a modal card.
class MessageDetailsModal extends StatelessWidget {
  const MessageDetailsModal({
    super.key,
    required this.preview,
    required this.versions,
  });

  final Widget preview;
  final Future<List<UiMessageVersion>> versions;

  @override
  Widget build(BuildContext context) {
    return ModalScaffold(
      title: AppLocalizations.of(context).messageDetails_title,
      onDismiss: () => Navigator.of(context).pop(),
      child: ModalBody(
        top: S.s16,
        child: MessageDetailsView(preview: preview, versions: versions),
      ),
    );
  }
}

/// The message as the chat shows it, followed by its edit history once the
/// versions are loaded and there is more than one.
class MessageDetailsView extends StatelessWidget {
  const MessageDetailsView({
    super.key,
    required this.preview,
    required this.versions,
  });

  final Widget preview;
  final Future<List<UiMessageVersion>> versions;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        preview,
        FutureBuilder(
          future: versions,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Padding(
                padding: const EdgeInsets.only(top: S.s24),
                child: Text(
                  AppLocalizations.of(context).messageDetails_error,
                  style: typeScale.body.s.style(
                    color: SemanticPalette.of(context).text.secondary,
                  ),
                ),
              );
            }
            final versions = snapshot.data ?? const [];
            // A message that was never edited has no history to show.
            if (versions.length < 2) return const SizedBox.shrink();
            return _EditHistory(versions: versions);
          },
        ),
      ],
    );
  }
}

/// Every version of an edited message, newest first, each with the time it
/// was sent or edited.
class _EditHistory extends StatelessWidget {
  const _EditHistory({required this.versions});

  /// Oldest first, as loaded.
  final List<UiMessageVersion> versions;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: S.s24, bottom: S.s8),
          child: Text(
            AppLocalizations.of(context).messageDetails_editHistory,
            style: typeScale.body.s.style(
              color: palette.text.secondary,
              weight: Weight.emphasized,
              tight: true,
            ),
          ),
        ),
        Column(
          crossAxisAlignment: .stretch,
          spacing: S.s16,
          children: [
            for (final (index, version) in versions.indexed.toList().reversed)
              _Version(version: version, isOriginal: index == 0),
          ],
        ),
      ],
    );
  }
}

class _Version extends StatelessWidget {
  const _Version({required this.version, required this.isOriginal});

  final UiMessageVersion version;
  final bool isOriginal;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final formats = TimeFormats.of(context);
    final createdAt = version.createdAt;
    final date = formats.monthDayYear(createdAt);
    final time = formats.clock(createdAt);
    // The history compares texts, not sides of the conversation. Every version
    // takes the outgoing look whoever sent the message, as its fill stands out
    // more from the card than the incoming one.
    const isSelf = true;
    final blocks = [
      for (final inner in version.content.content?.elements ?? [])
        buildBlockElement(context, inner.element, isSelf),
    ];

    return Column(
      crossAxisAlignment: .start,
      spacing: S.s4,
      children: [
        MessageBubble(
          isSelf: isSelf,
          child: MessageText(isSelf: isSelf, blocks: blocks),
        ),
        Padding(
          // Lines up with the text in the bubble, not with its edge.
          padding: EdgeInsets.symmetric(
            horizontal: MessageBubbleTokens.padding.left,
          ),
          child: Text(
            isOriginal
                ? loc.messageDetails_sent(date, time)
                : loc.messageDetails_edited(date, time),
            style: typeScale.body.s.style(
              color: SemanticPalette.of(context).text.tertiary,
            ),
          ),
        ),
      ],
    );
  }
}
