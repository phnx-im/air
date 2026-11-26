// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/features/attachments/attachment_upload_view.dart';
import 'package:air/features/chat/share_target_publisher.dart';
import 'package:air/features/emoji/emoji_data.dart';
import 'package:air/l10n/app_localizations_extension.dart';
import 'package:air/features/emoji/emoji_autocomplete.dart';
import 'package:air/features/message_list/mention_autocomplete.dart';
import 'package:air/ds/components/field/field_chrome.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/patterns/message_input/message_input.dart';
import 'package:air/ds/patterns/message_input/message_input_link_preview.dart';
import 'package:air/ds/patterns/message_input/message_input_media.dart';
import 'package:air/ds/patterns/message_input/message_input_quote.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/message_list/link_preview_data.dart';
import 'package:air/features/message_list/scroll_to_bottom_controller.dart';
import 'package:flutter_blurhash/flutter_blurhash.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/features/message_list/widgets/text_autocomplete.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';
import 'package:path_provider/path_provider.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/share/pending_share.dart';
import 'package:provider/provider.dart';

import 'package:air/platform/method_channel.dart'
    show ClipboardImage, getClipboardFilePaths, getClipboardImage;

import 'package:air/features/message_list/message_renderer.dart';

final _log = Logger("MessageComposer");

/// Where an attachment comes from. Camera is a phone-only source.
enum _AttachmentCategory { gallery, camera, file }

class MessageComposer extends StatefulWidget {
  const MessageComposer({
    super.key,
    this.scrollToBottomController,
    this.textEditingController,
  });

  final ScrollToBottomController? scrollToBottomController;
  final TextEditingController? textEditingController;

  @override
  State<MessageComposer> createState() => _MessageComposerState();
}

class _MessageComposerState extends State<MessageComposer>
    with WidgetsBindingObserver, TickerProviderStateMixin {
  late final TextEditingController _inputController;
  final Debouncer _storeDraftDebouncer = Debouncer(
    delay: const Duration(seconds: 1),
  );
  final Debouncer _linkPreviewDebouncer = Debouncer(
    delay: const Duration(milliseconds: 300),
  );

  /// Delays the pending link preview card, so a fetch that fails right away
  /// never flashes it.
  final Debouncer _linkPreviewLoadingDebouncer = Debouncer(
    delay: const Duration(milliseconds: 150),
  );
  StreamSubscription<ChatDetailsState>? _linkPreviewSubscription;
  String? _linkPreviewLoadingUrl;
  bool _showLinkPreviewLoading = false;

  StreamSubscription<ChatDetailsState>? _draftLoadingSubscription;
  final _focusNode = FocusNode();
  late ChatDetailsCubit _chatDetailsCubit;
  bool _inputIsEmpty = true;
  // A send keeps the staged attachments until the message is stored.
  bool _isSending = false;
  late final NavigationCubit _navigationCubit;
  StreamSubscription<NavigationState>? _navigationSubscription;
  String _inputTextCache = '';
  final LayerLink _inputFieldLink = LayerLink();
  final GlobalKey _inputFieldKey = GlobalKey();
  final GlobalKey _fieldChromeKey = GlobalKey();
  late final TextAutocompleteController<Emoji> _emojiAutocomplete;
  late final TextAutocompleteController<UiUserProfile> _mentionAutocomplete;

  /// Merged once rather than per build: a fresh merge on every rebuild makes
  /// [ListenableBuilder] drop and re-add its listeners each frame.
  late final Listenable _scrollBackState;

  @override
  void initState() {
    super.initState();
    _inputController =
        widget.textEditingController ?? CustomTextEditingController();
    WidgetsBinding.instance.addObserver(this);
    _emojiAutocomplete = TextAutocompleteController<Emoji>(
      textController: _inputController,
      focusNode: _focusNode,
      inputFieldKey: _inputFieldKey,
      anchorLink: _inputFieldLink,
      vsync: this,
      contextProvider: () => context,
      strategy: EmojiAutocompleteStrategy(),
    );
    _focusNode.addListener(_emojiAutocomplete.handleFocusChange);
    _focusNode.onKeyEvent = _onKeyEvent;
    _inputController.addListener(_onTextChanged);

    _scrollBackState = Listenable.merge([
      widget.scrollToBottomController?.showButton,
    ]);

    _chatDetailsCubit = context.read<ChatDetailsCubit>();

    _mentionAutocomplete = TextAutocompleteController<UiUserProfile>(
      textController: _inputController,
      focusNode: _focusNode,
      inputFieldKey: _inputFieldKey,
      anchorLink: _inputFieldLink,
      // The picker holds still above the field rather than chasing the caret.
      above: (key: _fieldChromeKey, gap: MessageInputTokens.mentionPickerGap),
      vsync: this,
      contextProvider: () => context,
      strategy: MentionAutocompleteStrategy(
        getMembers: () => _chatDetailsCubit.state.members,
        getProfile: (userId) =>
            context.read<UsersCubit>().state.profile(userId: userId),
        getOwnUserId: () => context.read<UserCubit>().state.userId,
        isEnabled: () =>
            context.read<UserSettingsCubit>().state.experimentalFeaturesActive,
      ),
      // Profiles load lazily; refresh the open picker when they arrive.
      refreshStream: context.read<UsersCubit>().stream,
    );
    _focusNode.addListener(_mentionAutocomplete.handleFocusChange);

    // Keep track of whether we loaded a draft for the first time
    bool isDraftLoaded = false;

    // Propagate loaded draft to the text field.
    //
    // There are two cases when the changes are propagated:
    //
    // 1. Initially loaded draft
    // 2. Editing ID has changed (when user clicks edit on another message)
    MessageId? currentEditingId;

    // Stage the reply we have in the draft.
    //
    // There are two cases when the changes are propagated:
    //
    // 1. Initially loaded draft
    // 2. In Reply To ID has changed (when user clicks reply on another message)
    UiMimiId? currentInReplyToId;

    _navigationCubit = context.read<NavigationCubit>();
    // A share can arrive for the chat that is already open.
    _navigationSubscription = _navigationCubit.stream.listen(
      (_) => _maybeApplyPendingShare(),
    );

    _onLinkPreviewChanged(_chatDetailsCubit.state.linkPreview);
    var hasStagedAttachments =
        _chatDetailsCubit.state.stagedAttachments.isNotEmpty;
    _linkPreviewSubscription = _chatDetailsCubit.stream.listen((state) {
      _onLinkPreviewChanged(state.linkPreview);
      // Links are not resolved while attachments are staged.
      final hasStaged = state.stagedAttachments.isNotEmpty;
      if (hasStaged != hasStagedAttachments) {
        hasStagedAttachments = hasStaged;
        _scheduleLinkPreviewUpdate(_inputController.text);
      }
    });

    _draftLoadingSubscription = _chatDetailsCubit.stream.listen((state) {
      if (state.chat == null) {
        return;
      }

      // always request focus on chat draft loading on desktop
      bool requestFocus = DeviceType.isDesktop;

      switch (state.chat?.draft) {
        // Initially loaded draft
        case final draft? when draft.isCommitted && !isDraftLoaded:
          isDraftLoaded = true;
          // if input is not empty, then the user already typed something,
          // and we don't want to overwrite it.
          if (_inputController.text.isEmpty) {
            _inputController.text = draft.message;
          }
          if (draft.message.isNotEmpty) {
            // open keyboard when a chat has a non-empty draft
            requestFocus = true;
          }
        // Editing ID has changed
        case final draft when draft?.editingId != currentEditingId:
          _inputController.text = draft?.message ?? "";
          currentEditingId = draft?.editingId;
          requestFocus = true; // open keyboard when switching edits
        // Reply ID has changed
        case final draft when draft?.inReplyTo?.$1 != currentInReplyToId:
          currentInReplyToId = draft?.inReplyTo?.$1;
          // we purposefully do not reset the already typed text, as we
          // only want to (re)set the reply.
          requestFocus = true; // open keyboard when switching reply to
        default:
      }

      _maybeApplyPendingShare();

      if (requestFocus) {
        _focusNode.requestFocus();
      }
    });
  }

  /// Stages the share navigation is holding for this chat: the shared text
  /// goes into the input field and the shared files run through the regular
  /// attachment upload preview, one after the other, as if picked in-app.
  void _maybeApplyPendingShare() {
    final chat = _chatDetailsCubit.state.chat;
    if (chat == null || !mounted) {
      return;
    }
    final loc = AppLocalizations.of(context);
    final navigationState = _navigationCubit.state;
    final share = navigationState.pendingShare;
    // The share is addressed by navigation, not by this widget: it belongs to
    // whichever chat navigation is pointed at.
    if (share == null ||
        navigationState.chatId != chat.id ||
        !chat.canShareInto) {
      return;
    }
    _navigationCubit.shareConsumed();
    if (share.droppedAttachments > 0) {
      showSnackBarStandalone(
        (loc) => loc.shareScreen_droppedItems(share.droppedAttachments),
        tone: .danger,
      );
    }
    final text = share.text;
    if (text != null && text.isNotEmpty) {
      final currentText = _inputController.text;
      final separator = currentText.isEmpty || currentText.endsWith('\n')
          ? ''
          : '\n';
      final updatedText = '$currentText$separator$text';
      _inputController.value = TextEditingValue(
        text: updatedText,
        selection: TextSelection.collapsed(offset: updatedText.length),
      );
    }
    if (share.attachments.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        unawaited(_stageSharedAttachments(share.attachments, chat.title(loc)));
      });
    }
  }

  /// Runs each shared file through the upload preview in turn. Backing out
  /// of a preview drops that file and the remaining ones.
  Future<void> _stageSharedAttachments(
    List<UiSharedAttachment> attachments,
    String chatTitle,
  ) async {
    for (var index = 0; index < attachments.length; index++) {
      final attachment = attachments[index];
      var uploaded = false;
      if (mounted) {
        uploaded = await _navigateToUploadPreview(
          context,
          XFile(attachment.path, mimeType: attachment.mimeType),
          isTempFile: true,
          chatTitle: chatTitle,
        );
      }
      if (!uploaded) {
        // An uploaded file is deleted by the cubit, a dropped one and the
        // not yet previewed rest are deleted here.
        await deleteShareFiles(attachments.sublist(index));
        return;
      }
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);

    _storeDraftDebouncer.dispose();
    _linkPreviewDebouncer.dispose();
    _linkPreviewLoadingDebouncer.dispose();
    _linkPreviewSubscription?.cancel();

    _chatDetailsCubit.storeDraft(
      draftMessage: _inputController.text.trim(),
      isCommitted: true,
    );

    _emojiAutocomplete.dispose();
    _focusNode.removeListener(_emojiAutocomplete.handleFocusChange);
    _mentionAutocomplete.dispose();
    _focusNode.removeListener(_mentionAutocomplete.handleFocusChange);
    _inputController.dispose();

    _draftLoadingSubscription?.cancel();
    _navigationSubscription?.cancel();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    // The dot on the scroll-back button tracks the chat's live unread count.
    // Mark-as-read runs up to the newest visible message, so whatever is still
    // unread sits below the fold.
    final (
      chatTitle,
      editingId,
      inReplyToId,
      isConfirmedChat,
      hasUnread,
      hasLinkPreview,
      hasStagedAttachments,
      canAttach,
    ) = context.select((ChatDetailsCubit cubit) {
      final chat = cubit.state.chat;
      return (
        chat?.title(loc),
        chat?.draft?.editingId,
        chat?.draft?.inReplyTo?.$1,
        chat?.isConfirmed ?? false,
        (chat?.unreadMessages ?? 0) > 0,
        switch (cubit.state.linkPreview) {
          null => false,
          UiComposerLinkPreview_Loading() => _showLinkPreviewLoading,
          UiComposerLinkPreview_Ready() => true,
        },
        cubit.state.stagedAttachments.isNotEmpty,
        cubit.state.linkPreview is! UiComposerLinkPreview_Ready,
      );
    });

    if (chatTitle == null) {
      return const SizedBox.shrink();
    }

    final isEditing = editingId != null;
    final controller = widget.scrollToBottomController;

    return SnackBarClearance(
      child: ListenableBuilder(
        listenable: _scrollBackState,
        builder: (context, _) => MessageInput(
          tokens: MessageInputTokens.current,
          // Cancel the edit when editing, attach otherwise.
          leadingIcon: isEditing ? AppIconType.x : AppIconType.plus,
          onLeading: isEditing
              ? (_) {
                  context.read<ChatDetailsCubit>().resetDraft();
                  _inputController.clear();
                }
              : isConfirmedChat && canAttach
              ? _openAttachMenu
              : null,
          sendIcon: isEditing ? AppIconType.check : AppIconType.arrowUp,
          // An edit keeps its confirm button whatever the field holds, so the way
          // out of an edit is in the same place the way in was. Send is never
          // shown disabled: the slot simply stays closed until there is something
          // to send.
          showSend:
              (isEditing || !_inputIsEmpty || hasStagedAttachments) &&
              isConfirmedChat,
          onSend: () => _submitMessage(context.read()),
          showScrollBack: controller?.showButton.value ?? false,
          scrollBackUnread: hasUnread,
          onScrollBack: () => controller?.scrollToBottom(),
          aboveField: [
            if (editingId != null) _EditBanner(editingId: editingId),
            if (inReplyToId != null)
              _ReplyPreview(last: !hasLinkPreview && !hasStagedAttachments),
            // Only present while staged, an empty row would cost the field its
            // top inset.
            if (hasLinkPreview) _LinkPreviewStaged(last: !hasStagedAttachments),
            if (hasStagedAttachments) const _StagedAttachments(),
          ],
          fieldKey: _fieldChromeKey,
          field: _ComposerField(
            focusNode: _focusNode,
            controller: _inputController,
            chatTitle: chatTitle,
            layerLink: _inputFieldLink,
            inputKey: _inputFieldKey,
            onSubmitMessage: () =>
                _submitMessage(context.read<ChatDetailsCubit>()),
            onImagePasted: _handleImagePaste,
            onFilePasted: _handleFilePaste,
            onContentInserted: _handleContentInserted,
          ),
        ),
      ),
    );
  }

  // Key events
  KeyEventResult _onKeyEvent(FocusNode node, KeyEvent evt) {
    final emojiResult = _emojiAutocomplete.handleKeyEvent(evt);
    if (emojiResult != null) {
      return emojiResult;
    }

    final mentionResult = _mentionAutocomplete.handleKeyEvent(evt);
    if (mentionResult != null) {
      return mentionResult;
    }

    // Intercept Cmd+V / Ctrl+V on desktop to handle image paste
    if (evt is KeyDownEvent &&
        evt.logicalKey == LogicalKeyboardKey.keyV &&
        !HardwareKeyboard.instance.isShiftPressed &&
        !HardwareKeyboard.instance.isAltPressed &&
        _isPasteModifierPressed) {
      _handleKeyboardPaste();
      return KeyEventResult.handled;
    }

    final modifierKeyPressed =
        HardwareKeyboard.instance.isShiftPressed ||
        HardwareKeyboard.instance.isAltPressed ||
        HardwareKeyboard.instance.isMetaPressed ||
        HardwareKeyboard.instance.isControlPressed;

    // On desktop, Enter always sends (Shift+Enter inserts newline via the
    // modifier check above). On mobile, ignore Enter key events entirely:
    // sending is handled by onEditingComplete (IME callback),
    // which hardware USB keyboards also use/go through on Android.
    if (!modifierKeyPressed &&
        evt.logicalKey == LogicalKeyboardKey.enter &&
        evt is KeyDownEvent &&
        DeviceType.isDesktop) {
      final chatDetailsCubit = context.read<ChatDetailsCubit>();
      _submitMessage(chatDetailsCubit);
      return KeyEventResult.handled;
    } else if (!modifierKeyPressed &&
        evt.logicalKey == LogicalKeyboardKey.arrowUp &&
        evt is KeyDownEvent) {
      final chatDetailsCubit = context.read<ChatDetailsCubit>();
      return _editMessage(chatDetailsCubit)
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    } else if (!modifierKeyPressed &&
        evt.logicalKey == LogicalKeyboardKey.escape &&
        evt is KeyDownEvent) {
      final chatDetailsCubit = context.read<ChatDetailsCubit>();
      return _resetDraft(chatDetailsCubit)
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    } else {
      return KeyEventResult.ignored;
    }
  }

  bool get _isPasteModifierPressed => Platform.isMacOS
      ? HardwareKeyboard.instance.isMetaPressed &&
            !HardwareKeyboard.instance.isControlPressed
      : HardwareKeyboard.instance.isControlPressed &&
            !HardwareKeyboard.instance.isMetaPressed;

  void _handleKeyboardPaste() async {
    // Check for file paths first (desktop only) — prevents macOS from
    // treating a copied file's icon as a pasted image.
    if (_canAttach) {
      final filePaths = await getClipboardFilePaths();
      if (filePaths != null && filePaths.isNotEmpty) {
        _handleFilePaste(filePaths.first);
        return;
      }

      final image = await getClipboardImage();
      if (image != null && image.bytes.isNotEmpty) {
        _handleImagePaste(image);
        return;
      }
    }
    // No image — fall back to text paste
    final clipData = await Clipboard.getData(Clipboard.kTextPlain);
    final text = clipData?.text;
    if (text != null && text.isNotEmpty) {
      final selection = _inputController.selection;
      final currentText = _inputController.text;
      final newText = currentText.replaceRange(
        selection.start,
        selection.end,
        text,
      );
      _inputController.value = TextEditingValue(
        text: newText,
        selection: TextSelection.collapsed(
          offset: selection.start + text.length,
        ),
      );
    }
  }

  void _submitMessage(ChatDetailsCubit chatDetailsCubit) async {
    if (_isSending) {
      return;
    }
    final messageText = _inputController.text.trim();
    // The staged attachments are sent with the text as their caption.
    final isEditing = chatDetailsCubit.state.chat?.draft?.editingId != null;
    final hasAttachments =
        !isEditing && chatDetailsCubit.state.stagedAttachments.isNotEmpty;
    // An empty edit removes the caption of an attachment message. The cubit
    // ignores it for any other message.
    if (messageText.isEmpty && !hasAttachments && !isEditing) {
      return;
    }

    widget.scrollToBottomController
      ?..scrollToBottom()
      ..sending.value = true;

    setState(() {
      _inputController.clear();
      _focusNode.requestFocus();
    });
    _storeDraftDebouncer.cancel();

    _isSending = true;
    try {
      await chatDetailsCubit.sendMessage(messageText);
      final chatId = chatDetailsCubit.state.chat?.id;
      if (chatId != null && mounted) {
        context.read<ShareTargetPublisher>().reportUsed(chatId);
      }
    } catch (e, stackTrace) {
      _log.severe("Failed to send message", e, stackTrace);
      // A refused edit leaves edit mode and keeps the text as a new message.
      showSnackBarStandalone(
        (loc) => switch (e) {
          UiSendMessageError_EditWindowPassed() =>
            loc.composer_error_editWindowPassed,
          UiSendMessageError_EditLimitReached() =>
            loc.composer_error_editLimitReached,
          UiSendMessageError_AttachmentDecoding() =>
            loc.composer_error_attachment_decoding,
          _ => loc.composer_error_sendMessage,
        },
        tone: .danger,
      );
      // Restore the text unless the user has started typing a new message.
      if (mounted && _inputController.text.trim().isEmpty) {
        setState(() {
          _inputController.text = messageText;
        });
      }
    } finally {
      _isSending = false;
      // Normally the arriving message has released it already.
      if (mounted) widget.scrollToBottomController?.sending.value = false;
    }
  }

  bool _editMessage(ChatDetailsCubit cubit) {
    // in case we already typed a message, do not start an edit
    // which would erase the text in the input field.
    if (_inputController.text.trim().isNotEmpty) {
      return false;
    }
    if (cubit.state.chat?.draft?.editingId != null) {
      return false;
    }
    cubit.editMessage();
    return true;
  }

  bool _resetDraft(ChatDetailsCubit cubit) {
    // One thing per press, from the field up: the last staged attachment,
    // then link preview, then reply, then edit.
    final linkUrl = switch (cubit.state.linkPreview) {
      UiComposerLinkPreview_Ready(:final card) => card.url,
      UiComposerLinkPreview_Loading(:final url) when _showLinkPreviewLoading =>
        url,
      _ => null,
    };
    if (cubit.state.stagedAttachments.lastOrNull case final staged?) {
      cubit.unstageAttachment(staged.attachmentId);
      return true;
    } else if (linkUrl != null) {
      cubit.dismissLinkPreview(linkUrl);
      return true;
    } else if (cubit.state.chat?.draft?.inReplyTo != null) {
      cubit.resetDraftReply();
      return true;
    } else if (cubit.state.chat?.draft?.editingId != null) {
      cubit.resetDraft();
      _inputController.clear();
      return true;
    }
    return false;
  }

  /// Opens the attach options as a floating menu growing out of the + button.
  void _openAttachMenu(BuildContext buttonContext) {
    final render = buttonContext.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;
    final loc = AppLocalizations.of(buttonContext);

    void pick(_AttachmentCategory category) =>
        unawaited(_pickAttachment(buttonContext, category));

    unawaited(
      showOverlayMenu(
        context: buttonContext,
        anchor: render.localToGlobal(Offset.zero) & render.size,
        // The button sits at the bottom of the screen, so the menu opens
        // upward out of its leading edge.
        corner: MenuCorner.bottomLeft,
        items: [
          if (DeviceType.isPhone)
            MenuItem(
              label: loc.attachment_camera,
              leading: const AppIcon.camera(size: 16),
              onPressed: () => pick(_AttachmentCategory.camera),
            ),
          MenuItem(
            label: loc.attachment_images,
            leading: const AppIcon.image(size: 16),
            onPressed: () => pick(_AttachmentCategory.gallery),
          ),
          MenuItem(
            label: loc.attachment_otherFiles,
            leading: const AppIcon.paperclip(size: 16),
            onPressed: () => pick(_AttachmentCategory.file),
          ),
        ],
      ),
    );
  }

  Future<void> _pickAttachment(
    BuildContext context,
    _AttachmentCategory category,
  ) async {
    // Note: using imageQuality triggers re-encoding, which loses animation
    // properties from GIFs or other animated formats.
    final XFile? file = switch (category) {
      .gallery => await ImagePicker().pickImage(source: .gallery),
      .camera => await ImagePicker().pickImage(source: .camera),
      .file => await openFile(),
    };

    if (file == null) {
      return;
    }

    if (!context.mounted) {
      return;
    }

    await _stageAttachment(file);
  }

  /// A message has either a link preview or attachments, so nothing can be
  /// attached once the composer has a link preview. One that is still loading
  /// gives way. An edit cannot add attachments either.
  bool get _canAttach {
    final state = _chatDetailsCubit.state;
    return state.linkPreview is! UiComposerLinkPreview_Ready &&
        state.chat?.draft?.editingId == null;
  }

  void _handleFilePaste(String filePath) {
    if (!_canAttach) return;

    final file = XFile(filePath);
    unawaited(_stageAttachment(file));
  }

  void _handleImagePaste(ClipboardImage image) async {
    if (!_canAttach) return;

    final ext = image.mimeType.split('/').last;
    final tempDir = await getTemporaryDirectory();
    final tempFile = File(_uniqueTempPath(tempDir, 'clipboard_paste', ext));
    await tempFile.writeAsBytes(image.bytes);
    final file = XFile(tempFile.path, mimeType: image.mimeType);

    if (!mounted) return;
    await _stageAttachment(file, isTempFile: true);
  }

  void _handleContentInserted(KeyboardInsertedContent content) async {
    final data = content.data;
    if (data == null || data.isEmpty) return;
    if (!_canAttach) return;

    final ext = content.mimeType.split('/').last;
    final tempDir = await getTemporaryDirectory();
    final tempFile = File(_uniqueTempPath(tempDir, 'keyboard_insert', ext));
    await tempFile.writeAsBytes(data);
    final file = XFile(tempFile.path, mimeType: content.mimeType);

    if (!mounted) return;
    await _stageAttachment(file, isTempFile: true);
  }

  /// Stages the file in the composer.
  Future<void> _stageAttachment(XFile file, {bool isTempFile = false}) async {
    final cubit = _chatDetailsCubit;
    try {
      // A temp file is deleted by the cubit from here on.
      final error = await cubit.stageAttachment(
        file.path,
        isTempFile: isTempFile,
      );
      _showAttachmentError(error);
    } catch (e) {
      _log.severe("Failed to stage attachment: $e", e);
      showSnackBarStandalone(
        (loc) => loc.composer_error_attachment,
        tone: .danger,
      );
    }
    if (mounted) _focusNode.requestFocus();
  }

  void _showAttachmentError(UploadAttachmentError? error) {
    switch (error) {
      case UploadAttachmentError_TooLarge(
        :final maxSizeBytes,
        :final actualSizeBytes,
      ):
        showSnackBarStandalone(
          (loc) => loc.composer_error_attachment_too_large(
            loc.bytesToHumanReadable(actualSizeBytes.toInt()),
            loc.bytesToHumanReadable(maxSizeBytes.toInt()),
          ),
          tone: .danger,
        );
      case UploadAttachmentError_DecodingError():
        showSnackBarStandalone(
          (loc) => loc.composer_error_attachment_decoding,
          tone: .danger,
        );
      case null:
    }
  }

  /// Returns whether the user confirmed the upload, as opposed to backing
  /// out of the preview.
  Future<bool> _navigateToUploadPreview(
    BuildContext context,
    XFile file, {
    bool isTempFile = false,
    required String chatTitle,
  }) async {
    final cubit = context.read<ChatDetailsCubit>();
    final shareTargetPublisher = context.read<ShareTargetPublisher>();

    var uploaded = false;
    await Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => AttachmentUploadView(
          title: chatTitle,
          file: file,
          onUpload: () async {
            uploaded = true;
            try {
              // A temp file is deleted by the cubit from here on.
              final error = await cubit.uploadAttachment(
                file.path,
                isTempFile: isTempFile,
              );
              _showAttachmentError(error);
              if (error == null) {
                final chatId = cubit.state.chat?.id;
                if (chatId != null) {
                  shareTargetPublisher.reportUsed(chatId);
                }
              }
            } catch (e) {
              _log.severe("Failed to upload attachment: $e", e);
              showSnackBarStandalone(
                (loc) => loc.composer_error_attachment,
                tone: .danger,
              );
            }
          },
        ),
      ),
    );

    if (isTempFile && !uploaded) {
      await _deleteTempFile(file.path);
    }
    return uploaded;
  }

  /// A sent file outlives the preview until it is uploaded, so each one
  /// needs a path of its own.
  String _uniqueTempPath(Directory dir, String prefix, String ext) {
    final suffix = DateTime.now().microsecondsSinceEpoch;
    return '${dir.path}/${prefix}_$suffix.$ext';
  }

  Future<void> _deleteTempFile(String path) async {
    try {
      await File(path).delete();
    } catch (e) {
      _log.warning("Failed to delete temp file: $e", e);
    }
  }

  void _onTextChanged() {
    final currentText = _inputController.text;
    if (currentText == _inputTextCache) {
      // Likely a selection or cursor position change, ignore.
      return;
    }
    _inputTextCache = currentText;

    setState(() {
      _inputIsEmpty = currentText.trim().isEmpty;
    });
    _storeDraftDebouncer.run(() {
      _chatDetailsCubit.storeDraft(
        draftMessage: currentText,
        isCommitted: false,
      );
    });
    _emojiAutocomplete.handleTextChanged();
    _mentionAutocomplete.handleTextChanged();
    _scheduleLinkPreviewUpdate(currentText);
  }

  /// Restarts the pending card's delay whenever the fetched address changes.
  void _onLinkPreviewChanged(UiComposerLinkPreview? preview) {
    final url = switch (preview) {
      UiComposerLinkPreview_Loading(:final url) => url,
      _ => null,
    };
    if (url == _linkPreviewLoadingUrl) return;
    _linkPreviewLoadingUrl = url;
    _linkPreviewLoadingDebouncer.cancel();
    if (_showLinkPreviewLoading) {
      setState(() => _showLinkPreviewLoading = false);
    }
    if (url == null) return;
    _linkPreviewLoadingDebouncer.run(() {
      if (mounted) setState(() => _showLinkPreviewLoading = true);
    });
  }

  /// Asks Rust to resolve a preview for the first link in [text].
  ///
  /// Experimental.
  void _scheduleLinkPreviewUpdate(String text) {
    _linkPreviewDebouncer.run(() {
      if (!mounted) return;
      final enabled = context
          .read<UserSettingsCubit>()
          .state
          .experimentalFeaturesActive;
      if (!enabled) {
        return;
      }
      _chatDetailsCubit
          .updateLinkPreviews(text)
          .catchError(
            (Object e, StackTrace st) =>
                _log.warning("Failed to update link previews", e, st),
          );
    });
  }
}

/// Edits left from which the banner counts them down.
const _editsLeftHintThreshold = 3;

/// The "Edit message" banner, shown above the field while an edit is staged.
/// Near the edit limit of the message, it tells how many edits are left.
class _EditBanner extends StatelessWidget {
  const _EditBanner({required this.editingId});

  final MessageId editingId;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    // The count is only known while the message is in the loaded window.
    final editsLeft = context.select((MessageListCubit cubit) {
      final message = cubit.state.messageById(editingId)?.message;
      return switch (message) {
        UiMessage_Content(:final field0) => field0.editsLeft,
        _ => null,
      };
    });
    final editsLeftHint = switch (editsLeft) {
      1 => loc.composer_lastEdit,
      final count? when count > 1 && count <= _editsLeftHintThreshold =>
        loc.composer_editsLeft(count),
      _ => null,
    };
    return Padding(
      padding: const EdgeInsets.only(
        top: S.s8,
        bottom: S.s8,
        left: MessageInputTokens.fieldPadding,
        right: MessageInputTokens.fieldPadding + S.s8,
      ),
      child: Text.rich(
        TextSpan(
          children: [
            WidgetSpan(
              alignment: .baseline,
              baseline: .alphabetic,
              child: AppIcon.pencil(size: S.s12, color: palette.text.tertiary),
            ),
            const WidgetSpan(child: SizedBox(width: S.s8)),
            TextSpan(text: loc.composer_editMessage),
            if (editsLeftHint != null) TextSpan(text: ' · $editsLeftHint'),
          ],
          style: typeScale.body.s.style(color: palette.text.tertiary),
        ),
      ),
    );
  }
}

/// The quoted message, shown above the field while a reply is staged.
class _ReplyPreview extends StatelessWidget {
  const _ReplyPreview({required this.last});

  /// Whether the field follows directly, see [MessageInputQuote.last].
  final bool last;

  @override
  Widget build(BuildContext context) {
    final inReplyTo = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.draft?.inReplyTo,
    );
    if (inReplyTo == null) return const SizedBox.shrink();
    final quote = quotedMessage(context, inReplyTo.$2);

    return MessageInputQuote(
      preview: quote.preview,
      senderName: quote.senderName,
      thumbnail: quotedThumbnail(context, inReplyTo.$2),
      last: last,
      onRemove: () => context.read<ChatDetailsCubit>().resetDraftReply(),
    );
  }
}

/// The attachments the next message is sent with, shown above the field.
class _StagedAttachments extends StatelessWidget {
  const _StagedAttachments();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final staged = context.select(
      (ChatDetailsCubit cubit) => cubit.state.stagedAttachments,
    );
    if (staged.isEmpty) return const SizedBox.shrink();

    return MessageInputMedia(
      items: [
        for (final attachment in staged)
          MessageInputMediaItem(
            imagePath: attachment.isImage ? attachment.path : null,
            fileName: attachment.filename,
            fileMeta: _fileMeta(loc, attachment),
          ),
      ],
      onRemove: (index) => context.read<ChatDetailsCubit>().unstageAttachment(
        staged[index].attachmentId,
      ),
    );
  }

  /// "{EXT} • {size}", without the extension when the name has none.
  String _fileMeta(AppLocalizations loc, UiStagedAttachment attachment) {
    final name = attachment.filename;
    final dot = name.lastIndexOf('.');
    final hasExtension = dot > 0 && dot < name.length - 1;
    return [
      if (hasExtension) name.substring(dot + 1).toUpperCase(),
      loc.bytesToHumanReadable(attachment.size.toInt()),
    ].join(' • ');
  }
}

/// The link preview of the draft, shown above the field once it resolves.
class _LinkPreviewStaged extends StatelessWidget {
  const _LinkPreviewStaged({required this.last});

  /// Whether the field follows directly, see [MessageInputLinkPreview.last].
  final bool last;

  @override
  Widget build(BuildContext context) {
    final preview = context.select(
      (ChatDetailsCubit cubit) => cubit.state.linkPreview,
    );
    if (preview == null) return const SizedBox.shrink();

    final cubit = context.read<ChatDetailsCubit>();
    return switch (preview) {
      UiComposerLinkPreview_Loading(:final url) => MessageInputLinkPreview(
        url: url,
        last: last,
        onRemove: () => cubit.dismissLinkPreview(url),
      ),
      UiComposerLinkPreview_Ready(:final card, :final image) =>
        MessageInputLinkPreview(
          url: card.url,
          data: card.toData(
            imageWidth: image?.width.toDouble(),
            imageHeight: image?.height.toDouble(),
          ),
          imageBuilder: image == null
              ? null
              : (fit) => _LinkPreviewImage(
                  url: card.url,
                  blurhash: image.blurhash,
                  fit: fit,
                ),
          last: last,
          onRemove: () => cubit.dismissLinkPreview(card.url),
        ),
    };
  }
}

/// The staged preview's image. Bytes stay in Rust and are fetched once per
/// [url], the blurhash shows until they load.
class _LinkPreviewImage extends HookWidget {
  const _LinkPreviewImage({
    required this.url,
    required this.blurhash,
    required this.fit,
  });

  final String url;
  final String? blurhash;
  final BoxFit fit;

  @override
  Widget build(BuildContext context) {
    final cubit = context.read<ChatDetailsCubit>();
    final future = useMemoized(() => cubit.composerLinkPreviewImage(url), [
      url,
    ]);
    final bytes = useFuture(future).data;
    final hash = blurhash;
    return Stack(
      fit: .expand,
      children: [
        ColoredBox(color: SemanticPalette.of(context).fill.tertiary),
        if (hash != null) BlurHash(hash: hash),
        if (bytes != null)
          Image.memory(
            bytes,
            fit: fit,
            gaplessPlayback: true,
            errorBuilder: (_, _, _) => const SizedBox.shrink(),
          ),
      ],
    );
  }
}

/// The composer's text field. Carries no decoration and no content padding:
/// [MessageInput] owns the chrome and insets it.
class _ComposerField extends StatelessWidget {
  const _ComposerField({
    required this.focusNode,
    required this.controller,
    required this.chatTitle,
    required this.layerLink,
    required this.inputKey,
    required this.onSubmitMessage,
    required this.onImagePasted,
    required this.onFilePasted,
    required this.onContentInserted,
  });

  final FocusNode focusNode;
  final TextEditingController controller;
  final String? chatTitle;
  final LayerLink layerLink;
  final GlobalKey inputKey;
  final VoidCallback onSubmitMessage;
  final ValueChanged<ClipboardImage> onImagePasted;
  final ValueChanged<String> onFilePasted;
  final ValueChanged<KeyboardInsertedContent> onContentInserted;

  @override
  Widget build(BuildContext context) {
    final sendOnEnter = context.select(
      (UserSettingsCubit cubit) => cubit.state.sendOnEnter,
    );

    final isConfirmedChat = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isConfirmed ?? false,
    );

    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return CompositedTransformTarget(
      key: inputKey,
      link: layerLink,
      child: TextField(
        focusNode: focusNode,
        controller: controller,
        style: typeScale.body.regular
            .style(color: palette.text.primary)
            .copyWith(leadingDistribution: .even),
        minLines: 1,
        maxLines: 10,
        enabled: isConfirmedChat,
        decoration: FieldChrome.plain(
          hintText: loc.composer_inputHint(chatTitle ?? ""),
          hintStyle: TextStyle(
            color: palette.text.tertiary,
            overflow: .ellipsis,
          ),
        ).copyWith(isCollapsed: true, hintMaxLines: 1),
        contextMenuBuilder: _contextMenuBuilder,
        textInputAction: sendOnEnter ? .send : .newline,
        onEditingComplete: sendOnEnter
            ? onSubmitMessage
            : () => focusNode.requestFocus(),
        keyboardType: TextInputType.multiline,
        textCapitalization: .sentences,
        contentInsertionConfiguration: ContentInsertionConfiguration(
          allowedMimeTypes: const [
            'image/gif',
            'image/webp',
            'image/png',
            'image/jpeg',
          ],
          onContentInserted: onContentInserted,
        ),
      ),
    );
  }

  // Custom context menu to handle image pasting from the clipboard. When the user
  // taps "Paste", we check if the clipboard contains image data.
  Widget _contextMenuBuilder(
    BuildContext context,
    EditableTextState editableTextState,
  ) {
    final existingItems = editableTextState.contextMenuButtonItems;
    final hasPaste = existingItems.any(
      (item) => item.type == ContextMenuButtonType.paste,
    );

    final items = existingItems.map((item) {
      if (item.type == ContextMenuButtonType.paste) {
        return ContextMenuButtonItem(
          label: item.label,
          type: item.type,
          onPressed: () async {
            // Check for file paths first (desktop only)
            final filePaths = await getClipboardFilePaths();
            if (filePaths != null && filePaths.isNotEmpty) {
              editableTextState.hideToolbar();
              onFilePasted(filePaths.first);
              return;
            }
            final image = await getClipboardImage();
            if (image != null && image.bytes.isNotEmpty) {
              editableTextState.hideToolbar();
              onImagePasted(image);
              return;
            }
            // No image — default text paste
            item.onPressed?.call();
          },
        );
      }
      return item;
    }).toList();

    // When the clipboard has image data but no text, Flutter omits the Paste
    // button on Android & iOS. Add one so the user can paste images or files.
    if (!hasPaste) {
      items.add(
        ContextMenuButtonItem(
          type: ContextMenuButtonType.paste,
          onPressed: () async {
            final filePaths = await getClipboardFilePaths();
            if (filePaths != null && filePaths.isNotEmpty) {
              editableTextState.hideToolbar();
              onFilePasted(filePaths.first);
              return;
            }
            final image = await getClipboardImage();
            if (image != null && image.bytes.isNotEmpty) {
              editableTextState.hideToolbar();
              onImagePasted(image);
            }
          },
        ),
      );
    }

    return AdaptiveTextSelectionToolbar.buttonItems(
      anchors: editableTextState.contextMenuAnchors,
      buttonItems: items,
    );
  }
}
