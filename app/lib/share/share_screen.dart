// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/core/core.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_stack.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/features/attachments/attachment_file_view.dart';
import 'package:air/features/chat_details/member_search_field.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/share/share_cubit.dart';
import 'package:air/share/share_payload.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// The two steps of the share flow.
///
/// They are separate pages rather than one screen because the sheet the OS
/// gives an extension is short: a picker sharing it with a preview and a
/// composer has room for two or three chats.
enum _ShareStep { destinations, compose }

/// Height of the preview strip, which is a thumbnail tall.
const double _stripHeight = 72;

/// Root widget of the share UI, creates the [IOSShareCubit]
class ShareScreen extends StatelessWidget {
  const ShareScreen({super.key, required this.payload, required this.dbPath});

  final SharePayload payload;
  final String dbPath;

  @override
  Widget build(BuildContext context) {
    return BlocProvider<IOSShareCubit>(
      create: (context) => IOSShareCubit(dbPath: dbPath),
      child: ShareScreenView(payload: payload),
    );
  }
}

class ShareScreenView extends StatefulWidget {
  const ShareScreenView({super.key, required this.payload});

  final SharePayload payload;

  @override
  State<ShareScreenView> createState() => _ShareScreenViewState();
}

class _ShareScreenViewState extends State<ShareScreenView> {
  final _searchController = TextEditingController();
  final _captionController = TextEditingController();

  /// The chats the content is sent to, in the order they were picked.
  final Set<ChatId> _selectedChatIds = {};
  String _query = '';

  _ShareStep _step = _ShareStep.destinations;
  bool _preselectionApplied = false;

  bool get _lostItems => widget.payload.droppedAttachments > 0;

  @override
  void dispose() {
    _searchController.dispose();
    _captionController.dispose();
    super.dispose();
  }

  void _applyPreselection(BuildContext context, ShareState state) {
    if (_preselectionApplied || !state.loaded || !state.signedIn) {
      return;
    }
    _preselectionApplied = true;
    final identifier = widget.payload.shareTargetIdentifier;
    if (identifier == null) {
      return;
    }
    final chatId = context.read<IOSShareCubit>().chatIdForShareTarget(
      identifier,
    );
    if (chatId == null) {
      return;
    }
    // A direct share target has picked its destination already, so the flow
    // opens on the compose step. Going back from there leads to the picker,
    // with the chat still selected.
    setState(() {
      _selectedChatIds.add(chatId);
      _step = _ShareStep.compose;
    });
  }

  void _toggleChat(ChatId chatId) {
    setState(() {
      if (!_selectedChatIds.remove(chatId)) {
        _selectedChatIds.add(chatId);
      }
    });
  }

  void _send(BuildContext context) {
    if (_selectedChatIds.isEmpty) {
      return;
    }
    final cubit = context.read<IOSShareCubit>();
    cubit.resetSendStatus();
    final message = [
      widget.payload.text,
      _captionController.text.trim(),
    ].nonNulls.where((text) => text.isNotEmpty).join('\n\n');
    // Content a chat already received is skipped by the cubit, so a retry
    // hands over the full selection again.
    cubit.send(
      chatIds: _selectedChatIds.toList(),
      attachments: widget.payload.attachments,
      message: message.isEmpty ? null : message,
    );
  }

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<IOSShareCubit, ShareState>(
      listener: (context, state) {
        _applyPreselection(context, state);
        // A share that lost items keeps the sheet open, so the notice about
        // them is seen rather than replaced by a silent success.
        if (state.sendStatus is UiShareSendStatus_Done && !_lostItems) {
          closeShareHost(success: true);
        }
      },
      builder: (context, state) {
        return ModalPageStack(
          // A send that is under way cannot be taken back to the picker.
          onBack: _sending(state.sendStatus)
              ? null
              : () => setState(() => _step = _ShareStep.destinations),
          onDismiss: () => closeShareHost(success: false),
          pages: [
            ModalStackEntry(
              key: const ValueKey('share-destinations'),
              child: _destinationsPane(context, state),
            ),
            if (_step == _ShareStep.compose)
              ModalStackEntry(
                key: const ValueKey('share-compose'),
                child: _composePane(context, state),
              ),
          ],
        );
      },
    );
  }

  /// The first step: pick the chats. It also carries the states that have
  /// nothing to pick for, so the flow always has a page below the compose one.
  Widget _destinationsPane(BuildContext context, ShareState state) {
    final loc = AppLocalizations.of(context);

    if (!state.loaded) {
      return _noticePane(loc, const CircularProgressIndicator());
    }
    if (!state.signedIn) {
      return _noticePane(loc, _SignedOutView(loc: loc));
    }
    // Nothing was handed over, so there is nothing to pick a chat for. Sending
    // an empty selection would otherwise close the sheet as a success.
    if (widget.payload.isEmpty) {
      return _noticePane(loc, _NothingToShareView(loc: loc));
    }

    return ModalPane(
      title: loc.shareScreen_title,
      dismissIcon: AppIconType.x,
      trailing: DialogHeaderAction(
        key: const Key('shareNextButton'),
        tokens: DialogHeaderTokens.of(context),
        icon: AppIconType.arrowRight,
        onPressed: _selectedChatIds.isEmpty
            ? null
            : () {
                FocusScope.of(context).unfocus();
                setState(() => _step = _ShareStep.compose);
              },
      ),
      // The picker below the search field scrolls on its own.
      scrollable: false,
      child: Column(
        children: [
          MemberSearchField(
            controller: _searchController,
            hintText: loc.shareScreen_searchHint,
            onChanged: (query) => setState(() => _query = query),
          ),
          Expanded(
            child: _ChatPickerList(
              chats: state.chats,
              query: _query,
              selectedChatIds: _selectedChatIds,
              onToggle: _toggleChat,
            ),
          ),
        ],
      ),
    );
  }

  /// A first step with nothing to pick, showing why.
  Widget _noticePane(AppLocalizations loc, Widget child) => ModalPane(
    title: loc.shareScreen_title,
    dismissIcon: AppIconType.x,
    scrollable: false,
    child: Center(child: child),
  );

  /// The second step: who the content goes to, what it is, and the message
  /// that goes with it.
  Widget _composePane(BuildContext context, ShareState state) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    // Idle and failed are the states a tap can act on. The rest have either
    // handed the content over already or are doing so.
    final sendable = switch (state.sendStatus) {
      UiShareSendStatus_Idle() ||
      UiShareSendStatus_Failed() => _selectedChatIds.isNotEmpty,
      _ => false,
    };
    final recipients = _recipients(loc, state.chats);

    return ModalPane(
      title: loc.shareScreen_title,
      // Where the way back is locked, the leading slot falls to the sheet's
      // own dismiss, which closes rather than goes back.
      dismissIcon: AppIconType.x,
      trailing: DialogHeaderAction(
        key: const Key('shareSendButton'),
        tokens: DialogHeaderTokens.of(context),
        icon: AppIconType.arrowUp,
        onPressed: sendable ? () => _send(context) : null,
      ),
      footer: _ComposeFooter(
        sendStatus: state.sendStatus,
        captionController: _captionController,
        confirmWhenSent: _lostItems,
      ),
      child: ModalBody(
        top: S.s16,
        child: Column(
          crossAxisAlignment: .start,
          children: [
            if (recipients != null)
              Text(
                recipients,
                style: typeScale.body.regular.style(
                  color: palette.text.secondary,
                ),
              ),
            if (_lostItems) ...[
              const SizedBox(height: S.s8),
              _DroppedItemsNotice(count: widget.payload.droppedAttachments),
            ],
            const SizedBox(height: S.s24),
            Center(child: _ContentPreview(payload: widget.payload)),
          ],
        ),
      ),
    );
  }

  /// The "To: …" line, naming the chats in the order they were picked. Past
  /// three it names two of them and counts the rest. Null where a selected
  /// chat has since left the list, which leaves nothing to name.
  String? _recipients(AppLocalizations loc, List<UiChatDetails> chats) {
    final names = [
      for (final chatId in _selectedChatIds)
        chats.where((chat) => chat.id == chatId).firstOrNull?.title,
    ].nonNulls.toList();
    if (names.isEmpty) {
      return null;
    }
    if (names.length > 3) {
      return loc.shareScreen_recipientsMore(
        names[0],
        names[1],
        names.length - 2,
      );
    }
    return loc.shareScreen_recipients(
      names.length,
      names.elementAtOrNull(0) ?? '',
      names.elementAtOrNull(1) ?? '',
      names.elementAtOrNull(2) ?? '',
    );
  }

  /// Whether the content is on its way, which locks the way back to the
  /// picker.
  bool _sending(UiShareSendStatus status) => switch (status) {
    UiShareSendStatus_Uploading() ||
    UiShareSendStatus_Sending() ||
    UiShareSendStatus_Done() => true,
    _ => false,
  };
}

class _SignedOutView extends StatelessWidget {
  const _SignedOutView({required this.loc});

  final AppLocalizations loc;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Padding(
      padding: const EdgeInsets.all(S.s24),
      // An iOS share extension can reach its host app only through a
      // registered URL scheme, which we do not want to expose app-wide for a
      // single button. So the message is all there is.
      child: Text(
        loc.shareScreen_signedOutMessage,
        textAlign: TextAlign.center,
        style: typeScale.body.regular.style(color: palette.text.primary),
      ),
    );
  }
}

/// Shown when the host could not hand over anything it was asked to share.
/// There is nothing to pick a chat for, so the only way out is closing.
class _NothingToShareView extends StatelessWidget {
  const _NothingToShareView({required this.loc});

  final AppLocalizations loc;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Padding(
      padding: const EdgeInsets.all(S.s24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            loc.shareScreen_nothingToShare,
            textAlign: TextAlign.center,
            style: typeScale.body.regular.style(color: palette.text.primary),
          ),
          const SizedBox(height: S.s16),
          OutlinedButton(
            onPressed: () => closeShareHost(success: false),
            child: Text(loc.shareScreen_close),
          ),
        ],
      ),
    );
  }
}

/// Tells how many of the shared items never made it into the payload. The
/// preview below shows what did.
class _DroppedItemsNotice extends StatelessWidget {
  const _DroppedItemsNotice({required this.count});

  final int count;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    return SizedBox(
      width: double.infinity,
      child: Text(
        loc.shareScreen_droppedItems(count),
        style: typeScale.body.xs.style(color: palette.function.danger),
      ),
    );
  }
}

class _ContentPreview extends StatelessWidget {
  const _ContentPreview({required this.payload});

  final SharePayload payload;

  @override
  Widget build(BuildContext context) {
    if (payload.attachments.isNotEmpty) {
      // The strip scrolls sideways, so what it holds gets no width from it.
      // The room the sheet leaves is what a file bubble lays its name out in.
      return LayoutBuilder(
        builder: (context, constraints) => SizedBox(
          height: _stripHeight,
          // Hugs its thumbnails, so a strip short enough to fit sits centered,
          // and scrolls once there are more of them than the width takes.
          child: ListView.separated(
            shrinkWrap: true,
            scrollDirection: Axis.horizontal,
            itemCount: payload.attachments.length,
            separatorBuilder: (_, _) => const SizedBox(width: S.s8),
            itemBuilder: (context, index) => _AttachmentPreview(
              attachment: payload.attachments[index],
              maxWidth: constraints.maxWidth,
            ),
          ),
        ),
      );
    }

    final text = payload.text;
    if (text == null || text.isEmpty) {
      return const SizedBox.shrink();
    }
    final palette = SemanticPalette.of(context);
    return Container(
      padding: const EdgeInsets.all(S.s12),
      decoration: BoxDecoration(
        color: palette.backgroundBase.secondary,
        borderRadius: BorderRadius.circular(CornerRadius.px12),
      ),
      child: Text(
        text,
        maxLines: 3,
        overflow: TextOverflow.ellipsis,
        style: typeScale.body.s.style(color: palette.text.secondary),
      ),
    );
  }
}

class _AttachmentPreview extends StatelessWidget {
  const _AttachmentPreview({required this.attachment, required this.maxWidth});

  final UiSharedAttachment attachment;
  final double maxWidth;

  bool get _isImage => attachment.mimeType?.startsWith('image/') ?? false;

  @override
  Widget build(BuildContext context) {
    if (_isImage) {
      // Decode at thumbnail size. A full-resolution camera photo decodes to
      // a bitmap larger than the extension's memory budget.
      final pixels = (_stripHeight * MediaQuery.devicePixelRatioOf(context))
          .round();
      return ClipRRect(
        borderRadius: BorderRadius.circular(CornerRadius.px12),
        child: Image(
          image: ResizeImage(
            FileImage(File(attachment.path)),
            width: pixels,
            height: pixels,
            policy: .fit,
            allowUpscaling: false,
          ),
          width: _stripHeight,
          height: _stripHeight,
          fit: BoxFit.cover,
          errorBuilder: (_, _, _) =>
              _FileBubble(attachment: attachment, maxWidth: maxWidth),
        ),
      );
    }
    return _FileBubble(attachment: attachment, maxWidth: maxWidth);
  }
}

/// Anything that is not a picture, in the bubble the chat gives it once it has
/// been sent.
class _FileBubble extends StatelessWidget {
  const _FileBubble({required this.attachment, required this.maxWidth});

  final UiSharedAttachment attachment;

  /// Width the name lays out in before it ellipsizes.
  final double maxWidth;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final file = File(attachment.path);
    final int fileSize;
    try {
      fileSize = file.lengthSync();
    } on FileSystemException {
      return const SizedBox.shrink();
    }

    // The strip is as tall as a thumbnail, which leaves the bubble room over.
    // A width factor keeps the alignment from claiming the unbounded width the
    // strip offers.
    return Align(
      widthFactor: 1,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: MessageBubble(
          isSelf: true,
          child: AttachmentFileView(
            leading: AppIcon.paperclip(
              size: S.s32,
              color: palette.message.selfText,
            ),
            filename: file.uri.pathSegments.lastOrNull ?? attachment.path,
            size: fileSize,
            color: palette.message.selfText,
            maxLines: 1,
          ),
        ),
      ),
    );
  }
}

/// Multi-select list of the chats the content can be sent to.
class _ChatPickerList extends StatelessWidget {
  const _ChatPickerList({
    required this.chats,
    required this.query,
    required this.selectedChatIds,
    required this.onToggle,
  });

  final List<UiChatDetails> chats;
  final String query;
  final Set<ChatId> selectedChatIds;
  final ValueChanged<ChatId> onToggle;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    final normalizedQuery = query.trim().toLowerCase();
    final filteredChats = normalizedQuery.isEmpty
        ? chats
        : chats
              .where(
                (chat) => chat.title.toLowerCase().contains(normalizedQuery),
              )
              .toList();

    if (filteredChats.isEmpty) {
      return Center(
        child: Text(
          loc.shareScreen_noChats,
          textAlign: TextAlign.center,
          style: typeScale.body.regular.style(color: palette.text.tertiary),
        ),
      );
    }

    return ListView.separated(
      // The surface runs to the bottom of the sheet, so the last row clears
      // the home indicator on its own rather than through a SafeArea.
      padding: EdgeInsets.only(
        bottom: MediaQuery.viewPaddingOf(context).bottom,
      ),
      itemCount: filteredChats.length,
      separatorBuilder: (context, index) => _Separator(
        // The fills of two selected rows reach the separator, so between
        // them it takes the background color to keep the rows apart.
        betweenSelected:
            selectedChatIds.contains(filteredChats[index].id) &&
            selectedChatIds.contains(filteredChats[index + 1].id),
      ),
      itemBuilder: (context, index) {
        final chat = filteredChats[index];
        return _ChatTile(
          chat: chat,
          selected: selectedChatIds.contains(chat.id),
          onTap: () => onToggle(chat.id),
        );
      },
    );
  }
}

class _Separator extends StatelessWidget {
  const _Separator({required this.betweenSelected});

  final bool betweenSelected;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return betweenSelected
        ? Container(
            height: StrokeWidth.px1,
            color: palette.backgroundBase.primary,
          )
        : Divider(
            height: StrokeWidth.px0_5,
            thickness: StrokeWidth.px0_5,
            indent: S.s16,
            endIndent: S.s16,
            color: palette.separator.secondary,
          );
  }
}

/// One destination in the picker.
///
/// A selected row carries a square fill running the full width, so a run of
/// them reads as one block of recipients.
class _ChatTile extends StatelessWidget {
  const _ChatTile({required this.chat, required this.selected, this.onTap});

  final UiChatDetails chat;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return ListRow(
      tokens: ListRowTokens.current,
      label: chat.title,
      leading: ChatDetailsAvatar(chat: chat, size: S.s40),
      trailing: selected
          ? AppIcon.check(size: S.s20, color: palette.accentBrand.primary)
          : null,
      fill: selected ? palette.fill.tertiary : null,
      radius: CornerRadius.px0,
      separator: false,
      onTap: onTap,
    );
  }
}

/// What sits below the compose step: the message field, or the progress and
/// the outcome of a send that is already under way.
class _ComposeFooter extends StatelessWidget {
  const _ComposeFooter({
    required this.sendStatus,
    required this.captionController,
    required this.confirmWhenSent,
  });

  final UiShareSendStatus sendStatus;
  final TextEditingController captionController;

  /// Whether a finished send waits for the person to close the sheet instead
  /// of closing itself.
  final bool confirmWhenSent;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    final (progressLabel, progressValue) = switch (sendStatus) {
      UiShareSendStatus_Uploading(
        :final current,
        :final total,
        :final progress,
      ) =>
        (loc.shareScreen_uploading(current, total), progress.clamp(0.0, 1.0)),
      UiShareSendStatus_Sending() => (loc.shareScreen_sending, null),
      // Done keeps the spinner up while the sheet closes itself.
      UiShareSendStatus_Done() when !confirmWhenSent => (
        loc.shareScreen_sending,
        null,
      ),
      _ => (null, null),
    };
    if (progressLabel != null) {
      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: S.s32,
            height: S.s32,
            child: CircularProgressIndicator(
              strokeWidth: 2,
              valueColor: AlwaysStoppedAnimation<Color>(palette.text.primary),
              backgroundColor: Colors.transparent,
              value: progressValue,
            ),
          ),
          const SizedBox(height: S.s8),
          Text(
            progressLabel,
            style: typeScale.body.s.style(color: palette.text.secondary),
          ),
        ],
      );
    }

    final queued = sendStatus is UiShareSendStatus_Queued;
    if (queued || sendStatus is UiShareSendStatus_Done) {
      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (queued) ...[
            Text(
              loc.shareScreen_queued,
              textAlign: TextAlign.center,
              style: typeScale.body.s.style(color: palette.text.secondary),
            ),
            const SizedBox(height: S.s8),
          ],
          OutlinedButton(
            onPressed: () => closeShareHost(success: true),
            child: Text(loc.shareScreen_done),
          ),
        ],
      );
    }

    final errorLabel = switch (sendStatus) {
      UiShareSendStatus_Failed(
        error: UiShareSendError_AttachmentTooLarge(
          :final maxSizeBytes,
          :final actualSizeBytes,
        ),
      ) =>
        loc.composer_error_attachment_too_large(
          loc.bytesToHumanReadable(actualSizeBytes.toInt()),
          loc.bytesToHumanReadable(maxSizeBytes.toInt()),
        ),
      UiShareSendStatus_Failed(
        error: UiShareSendError_TooManyAttachments(:final max),
      ) =>
        loc.shareScreen_tooManyAttachments(max),
      UiShareSendStatus_Failed() => loc.shareScreen_sendFailed,
      _ => null,
    };

    final captionBorder = OutlineInputBorder(
      borderRadius: BorderRadius.circular(CornerRadius.px12),
      borderSide: BorderSide(
        color: palette.separator.primary,
        width: StrokeWidth.px1,
      ),
    );

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (errorLabel != null) ...[
          Text(
            errorLabel,
            textAlign: TextAlign.center,
            style: typeScale.body.s.style(color: palette.function.danger),
          ),
          const SizedBox(height: S.s8),
        ],
        TextField(
          controller: captionController,
          textInputAction: TextInputAction.done,
          decoration: InputDecoration(
            isDense: true,
            hintText: loc.shareScreen_captionHint,
            hintStyle: Theme.of(
              context,
            ).textTheme.bodyMedium?.copyWith(color: palette.text.quaternary),
            // The field is the only thing on the step to type into, so focus
            // does not thicken or color the outline. Every state is named
            // because Material's focused border outranks a bare `border`.
            border: captionBorder,
            enabledBorder: captionBorder,
            focusedBorder: captionBorder,
          ),
        ),
      ],
    );
  }
}
