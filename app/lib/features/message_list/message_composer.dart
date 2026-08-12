// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';
import 'package:air/features/attachments/attachment_upload_view.dart';
import 'package:air/features/emoji/emoji_data.dart';
import 'package:air/l10n/app_localizations_extension.dart';
import 'package:air/features/emoji/emoji_autocomplete.dart';
import 'package:air/ds/components/field/field_chrome.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/patterns/message_input/message_input.dart';
import 'package:air/ds/patterns/message_input/message_input_quote.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/message_list/scroll_to_bottom_controller.dart';
import 'package:air/features/user/user_settings_cubit.dart';
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
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart' show AppLocalizations;
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
  StreamSubscription<ChatDetailsState>? _draftLoadingSubscription;
  final _focusNode = FocusNode();
  late ChatDetailsCubit _chatDetailsCubit;
  bool _inputIsEmpty = true;
  String _inputTextCache = '';
  final LayerLink _inputFieldLink = LayerLink();
  final GlobalKey _inputFieldKey = GlobalKey();
  late final TextAutocompleteController<Emoji> _emojiAutocomplete;

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

      if (requestFocus) {
        _focusNode.requestFocus();
      }
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);

    _storeDraftDebouncer.dispose();

    _chatDetailsCubit.storeDraft(
      draftMessage: _inputController.text.trim(),
      isCommitted: true,
    );

    _emojiAutocomplete.dispose();
    _focusNode.removeListener(_emojiAutocomplete.handleFocusChange);
    _inputController.dispose();

    _draftLoadingSubscription?.cancel();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // The dot on the scroll-back button tracks the chat's live unread count.
    // Mark-as-read runs up to the newest visible message, so whatever is still
    // unread sits below the fold.
    final (
      chatTitle,
      editingId,
      inReplyToId,
      isConfirmedChat,
      hasUnread,
    ) = context.select((ChatDetailsCubit cubit) {
      final chat = cubit.state.chat;
      return (
        chat?.title,
        chat?.draft?.editingId,
        chat?.draft?.inReplyTo?.$1,
        chat?.isConfirmed ?? false,
        (chat?.unreadMessages ?? 0) > 0,
      );
    });

    if (chatTitle == null) {
      return const SizedBox.shrink();
    }

    final isEditing = editingId != null;
    final controller = widget.scrollToBottomController;

    return ListenableBuilder(
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
            : isConfirmedChat
            ? (buttonContext) =>
                  _openAttachMenu(buttonContext, chatTitle: chatTitle)
            : null,
        sendIcon: isEditing ? AppIconType.check : AppIconType.arrowUp,
        // An edit keeps its confirm button whatever the field holds, so the way
        // out of an edit is in the same place the way in was. Send is never
        // shown disabled: the slot simply stays closed until there is something
        // to send.
        showSend: (isEditing || !_inputIsEmpty) && isConfirmedChat,
        onSend: () => _submitMessage(context.read()),
        showScrollBack: controller?.showButton.value ?? false,
        scrollBackUnread: hasUnread,
        onScrollBack: () => controller?.scrollToBottom(),
        aboveField: [
          if (isEditing) const _EditBanner(),
          if (inReplyToId != null) const _ReplyPreview(),
        ],
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
    );
  }

  // Key events
  KeyEventResult _onKeyEvent(FocusNode node, KeyEvent evt) {
    final emojiResult = _emojiAutocomplete.handleKeyEvent(evt);
    if (emojiResult != null) {
      return emojiResult;
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
    final messageText = _inputController.text.trim();
    if (messageText.isEmpty) {
      return;
    }

    widget.scrollToBottomController?.scrollToBottom();

    setState(() {
      _inputController.clear();
      _focusNode.requestFocus();
    });
    _storeDraftDebouncer.cancel();

    try {
      await chatDetailsCubit.sendMessage(messageText);
    } catch (e, stackTrace) {
      _log.severe("Failed to send message", e, stackTrace);
      showSnackBarStandalone(
        (loc) => SnackBar(content: Text(loc.composer_error_sendMessage)),
        tone: SnackbarTone.danger,
      );
      // Restore the text unless the user has started typing a new message.
      if (mounted && _inputController.text.trim().isEmpty) {
        setState(() {
          _inputController.text = messageText;
        });
      }
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
    // if we are replying to a message, reset only this
    if (cubit.state.chat?.draft?.inReplyTo != null) {
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
  void _openAttachMenu(
    BuildContext buttonContext, {
    required String chatTitle,
  }) {
    final render = buttonContext.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;
    final loc = AppLocalizations.of(buttonContext);

    void pick(_AttachmentCategory category) => unawaited(
      _pickAttachment(buttonContext, category, chatTitle: chatTitle),
    );

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
    _AttachmentCategory category, {
    required String chatTitle,
  }) async {
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

    _navigateToUploadPreview(context, file, chatTitle: chatTitle);
  }

  void _handleFilePaste(String filePath) {
    final chatTitle = _chatDetailsCubit.state.chat?.title;
    if (chatTitle == null) return;

    final file = XFile(filePath);
    _navigateToUploadPreview(context, file, chatTitle: chatTitle);
  }

  void _handleImagePaste(ClipboardImage image) async {
    final chatTitle = _chatDetailsCubit.state.chat?.title;
    if (chatTitle == null) return;

    final ext = image.mimeType.split('/').last;
    final tempDir = await getTemporaryDirectory();
    final tempFile = File('${tempDir.path}/clipboard_paste.$ext');
    await tempFile.writeAsBytes(image.bytes);
    final file = XFile(tempFile.path, mimeType: image.mimeType);

    if (!mounted) return;
    await _navigateToUploadPreview(
      context,
      file,
      chatTitle: chatTitle,
      isTempFile: true,
    );
  }

  void _handleContentInserted(KeyboardInsertedContent content) async {
    final data = content.data;
    if (data == null || data.isEmpty) return;

    final chatTitle = _chatDetailsCubit.state.chat?.title;
    if (chatTitle == null) return;

    final ext = content.mimeType.split('/').last;
    final tempDir = await getTemporaryDirectory();
    final tempFile = File('${tempDir.path}/keyboard_insert.$ext');
    await tempFile.writeAsBytes(data);
    final file = XFile(tempFile.path, mimeType: content.mimeType);

    if (!mounted) return;
    await _navigateToUploadPreview(
      context,
      file,
      chatTitle: chatTitle,
      isTempFile: true,
    );
  }

  Future<void> _navigateToUploadPreview(
    BuildContext context,
    XFile file, {
    bool isTempFile = false,
    required String chatTitle,
  }) {
    final cubit = context.read<ChatDetailsCubit>();

    return Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => AttachmentUploadView(
          title: chatTitle,
          file: file,
          onUpload: () async {
            try {
              final error = await cubit.uploadAttachment(file.path);
              switch (error) {
                case UploadAttachmentError_TooLarge(
                  :final maxSizeBytes,
                  :final actualSizeBytes,
                ):
                  showSnackBarStandalone(
                    (loc) => SnackBar(
                      content: Text(
                        loc.composer_error_attachment_too_large(
                          loc.bytesToHumanReadable(actualSizeBytes.toInt()),
                          loc.bytesToHumanReadable(maxSizeBytes.toInt()),
                        ),
                      ),
                    ),
                  );
                  break;
                case null:
                  break;
              }
            } catch (e) {
              _log.severe("Failed to upload attachment", e);
              showErrorBannerStandalone((loc) => loc.composer_error_attachment);
            } finally {
              if (isTempFile) {
                try {
                  await File(file.path).delete();
                } catch (e) {
                  _log.warning("Failed to delete temp file: $e", e);
                }
              }
            }
          },
        ),
      ),
    );
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
  }
}

/// The "Edit message" banner, shown above the field while an edit is staged.
class _EditBanner extends StatelessWidget {
  const _EditBanner();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
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
              alignment: PlaceholderAlignment.baseline,
              baseline: TextBaseline.alphabetic,
              child: AppIcon.pencil(size: S.s12, color: palette.text.tertiary),
            ),
            const WidgetSpan(child: SizedBox(width: S.s8)),
            TextSpan(text: loc.composer_editMessage),
          ],
          style: typeScale.body.s.style(color: palette.text.tertiary),
        ),
      ),
    );
  }
}

/// The quoted message, shown above the field while a reply is staged.
class _ReplyPreview extends StatelessWidget {
  const _ReplyPreview();

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
      onRemove: () => context.read<ChatDetailsCubit>().resetDraftReply(),
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
            .copyWith(leadingDistribution: TextLeadingDistribution.even),
        minLines: 1,
        maxLines: 10,
        enabled: isConfirmedChat,
        decoration: FieldChrome.plain(
          hintText: loc.composer_inputHint(chatTitle ?? ""),
          hintStyle: TextStyle(
            color: palette.text.tertiary,
            overflow: TextOverflow.ellipsis,
          ),
        ).copyWith(isCollapsed: true, hintMaxLines: 1),
        contextMenuBuilder: _contextMenuBuilder,
        textInputAction: sendOnEnter
            ? TextInputAction.send
            : TextInputAction.newline,
        onEditingComplete: sendOnEnter
            ? onSubmitMessage
            : () => focusNode.requestFocus(),
        keyboardType: TextInputType.multiline,
        textCapitalization: TextCapitalization.sentences,
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
