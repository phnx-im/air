// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';
import 'dart:ui';

import 'package:air/attachments/attachments.dart';
import 'package:air/l10n/app_localizations_extension.dart';
import 'package:air/message_list/emoji_repository.dart';
import 'package:air/message_list/emoji_autocomplete.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/message_list/scroll_to_bottom_controller.dart';
import 'package:air/user/user_settings_cubit.dart';
import 'package:air/user/users_cubit.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/message_list/widgets/text_autocomplete.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';
import 'package:path_provider/path_provider.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/effects/elevation.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:provider/provider.dart';

import 'package:air/util/platform.dart'
    show getClipboardFilePaths, getClipboardImage, PlatformExtension;

import 'message_renderer.dart';
import 'text_message_tile.dart' show messageHorizontalPadding;

final _log = Logger("MessageComposer");
const double _composerLineHeight = 1.3;
final double _composerFontSize = BodyFontSize.base.size;
const double _inputVerticalPadding = Spacings.xs;
final double _composerButtonSize =
    _composerFontSize * _composerLineHeight + 2 * _inputVerticalPadding;

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
  late final TextAutocompleteController<EmojiEntry> _emojiAutocomplete;

  static final double _buttonSize = _composerButtonSize;
  static final double _inputBorderRadius = _buttonSize / 2;
  static const double _iconSize = 16;

  @override
  void initState() {
    super.initState();
    _inputController =
        widget.textEditingController ?? CustomTextEditingController();
    WidgetsBinding.instance.addObserver(this);
    _emojiAutocomplete = TextAutocompleteController<EmojiEntry>(
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
      bool requestFocus = PlatformExtension.isDesktop;

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
    final (chatTitle, editingId, inReplyToId, isConfirmedChat) = context.select(
      (ChatDetailsCubit cubit) {
        final chat = cubit.state.chat;
        return (
          chat?.title,
          chat?.draft?.editingId,
          chat?.draft?.inReplyTo?.$1,
          chat?.isConfirmed ?? false,
        );
      },
    );

    if (chatTitle == null) {
      return const SizedBox.shrink();
    }

    final color = CustomColorScheme.of(context);
    final materialColor = color.material.tertiary;

    Widget composerButton({required Widget icon, VoidCallback? onPressed}) {
      return SizedBox(
        width: _buttonSize,
        height: _buttonSize,
        child: ClipOval(
          child: Container(
            decoration: BoxDecoration(
              color: materialColor,
              shape: BoxShape.circle,
            ),
            child: IconButton(
              icon: icon,
              color: color.text.primary,
              hoverColor: const Color(0x00FFFFFF),
              padding: EdgeInsets.zero,
              constraints: BoxConstraints.tightFor(
                width: _buttonSize,
                height: _buttonSize,
              ),
              onPressed: onPressed,
            ),
          ),
        ),
      );
    }

    final plusButton = composerButton(
      icon: const AppIcon.plus(size: _iconSize),
      onPressed: isConfirmedChat
          ? () => _uploadAttachment(context, chatTitle: chatTitle)
          : null,
    );

    final inputField = Expanded(
      child: ClipRRect(
        borderRadius: BorderRadius.circular(_inputBorderRadius),
        child: Container(
          constraints: BoxConstraints(minHeight: _buttonSize),
          decoration: BoxDecoration(
            color: materialColor,
            borderRadius: BorderRadius.circular(_inputBorderRadius),
          ),
          padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
          child: _MessageInput(
            focusNode: _focusNode,
            controller: _inputController,
            chatTitle: chatTitle,
            isEditing: editingId != null,
            isReplying: inReplyToId != null,
            layerLink: _inputFieldLink,
            inputKey: _inputFieldKey,
            onSubmitMessage: () =>
                _submitMessage(context.read<ChatDetailsCubit>()),
            onImagePasted: _handleImagePaste,
            onFilePasted: _handleFilePaste,
          ),
        ),
      ),
    );

    return Padding(
      padding: const EdgeInsets.only(
        left: messageHorizontalPadding,
        right: messageHorizontalPadding,
        bottom: Spacings.xs,
      ),
      child: ValueListenableBuilder<bool>(
        valueListenable:
            widget.scrollToBottomController?.showButton ??
            ValueNotifier<bool>(false),
        builder: (context, showScrollToBottom, _) {
          final isEditing = editingId != null;

          // Left: cancel (✕) when editing, attach (+) otherwise
          final leftButton = isEditing
              ? composerButton(
                  icon: const AppIcon.x(size: _iconSize),
                  onPressed: () {
                    context.read<ChatDetailsCubit>().resetDraft();
                    _inputController.clear();
                  },
                )
              : plusButton;

          // Right: confirm (✓) when editing, scroll-down when
          // scrolled back, send (↑) when has text, none otherwise
          final Widget? rightButton;
          if (isEditing) {
            rightButton = composerButton(
              icon: const AppIcon.check(size: _iconSize),
              onPressed: !_inputIsEmpty && isConfirmedChat
                  ? () => _submitMessage(context.read())
                  : null,
            );
          } else if (showScrollToBottom) {
            rightButton = composerButton(
              icon: const AppIcon.chevronsDown(size: _iconSize),
              onPressed: () {
                widget.scrollToBottomController?.scrollToBottom();
              },
            );
          } else if (!_inputIsEmpty) {
            rightButton = composerButton(
              icon: const AppIcon.arrowUp(size: _iconSize),
              onPressed: isConfirmedChat
                  ? () => _submitMessage(context.read())
                  : null,
            );
          } else {
            rightButton = null;
          }

          final trailingButtonCount = rightButton != null ? 1 : 0;

          final clipper = _ComposerClipper(
            buttonSize: _buttonSize,
            spacing: Spacings.xxs,
            inputBorderRadius: _inputBorderRadius,
            trailingButtonCount: trailingButtonCount,
          );

          return Stack(
            children: [
              Positioned.fill(
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final clipBounds = clipper
                        .getClip(constraints.smallest)
                        .getBounds();
                    return ClipPath(
                      clipper: clipper,
                      child: BackdropFilter(
                        filter: ImageFilter.blur(
                          sigmaX: 40,
                          sigmaY: 40,
                          bounds: clipBounds,
                        ),
                        child: const SizedBox.expand(),
                      ),
                    );
                  },
                ),
              ),
              Positioned.fill(
                child: CustomPaint(
                  painter: _ComposerShadowPainter(
                    buttonSize: _buttonSize,
                    spacing: Spacings.xxs,
                    inputBorderRadius: _inputBorderRadius,
                    trailingButtonCount: trailingButtonCount,
                  ),
                ),
              ),
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                spacing: Spacings.xxs,
                children: [leftButton, inputField, ?rightButton],
              ),
            ],
          );
        },
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

    if (!modifierKeyPressed &&
        evt.logicalKey == LogicalKeyboardKey.enter &&
        evt is KeyDownEvent) {
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

    final imageBytes = await getClipboardImage();
    if (imageBytes != null && imageBytes.isNotEmpty) {
      _handleImagePaste(imageBytes);
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

    // FIXME: Handle errors
    chatDetailsCubit.sendMessage(messageText);

    widget.scrollToBottomController?.scrollToBottom();

    setState(() {
      _inputController.clear();
      _focusNode.requestFocus();
    });
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

  void _uploadAttachment(
    BuildContext context, {
    required String chatTitle,
  }) async {
    AttachmentCategory? selectedCategory;
    await showBottomSheetModal(
      context: context,
      builder: (_) => AttachmentCategoryPicker(
        onCategorySelected: (category) {
          selectedCategory = category;
          Navigator.of(context).pop(true);
        },
      ),
    );

    // Reduce image quality to re-encode the image.
    final XFile? file = switch (selectedCategory) {
      .gallery => await ImagePicker().pickImage(
        source: .gallery,
        imageQuality: 99,
      ),
      .camera => await ImagePicker().pickImage(
        source: .camera,
        imageQuality: 99,
      ),
      .file => await openFile(),
      null => null,
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

  void _handleImagePaste(Uint8List imageBytes) async {
    final chatTitle = _chatDetailsCubit.state.chat?.title;
    if (chatTitle == null) return;

    final tempDir = await getTemporaryDirectory();
    final tempFile = File('${tempDir.path}/clipboard_paste.jpg');
    await tempFile.writeAsBytes(imageBytes);
    final file = XFile(tempFile.path);

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
              _log.severe("Failed to upload attachment: $e", e);
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

class _MessageInput extends StatelessWidget {
  const _MessageInput({
    required FocusNode focusNode,
    required TextEditingController controller,
    required this.chatTitle,
    required this.isEditing,
    required this.isReplying,
    required this.layerLink,
    required this.inputKey,
    required this.onSubmitMessage,
    required this.onImagePasted,
    required this.onFilePasted,
  }) : _focusNode = focusNode,
       _controller = controller;

  final FocusNode _focusNode;
  final TextEditingController _controller;
  final String? chatTitle;
  final bool isEditing;
  final bool isReplying;
  final LayerLink layerLink;
  final GlobalKey inputKey;
  final VoidCallback onSubmitMessage;
  final ValueChanged<Uint8List> onImagePasted;
  final ValueChanged<String> onFilePasted;

  @override
  Widget build(BuildContext context) {
    final sendOnEnter = context.select(
      (UserSettingsCubit cubit) => cubit.state.sendOnEnter,
    );

    final isConfirmedChat = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isConfirmed ?? false,
    );

    final (isEditing, inReplyTo) = context.select(
      (ChatDetailsCubit cubit) => (
        cubit.state.chat?.draft?.editingId != null,
        cubit.state.chat?.draft?.inReplyTo,
      ),
    );

    final loc = AppLocalizations.of(context);
    final color = CustomColorScheme.of(context);

    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (isEditing)
          Padding(
            padding: const EdgeInsets.only(
              top: Spacings.xs,
              left: Spacings.xxs,
              right: Spacings.xxs,
            ),
            child: Row(
              children: [
                AppIcon.pencil(
                  size: 20,
                  color: CustomColorScheme.of(context).text.tertiary,
                ),
                const SizedBox(width: Spacings.xxs),
                Text(
                  loc.composer_editMessage,
                  style: TextStyle(
                    fontSize: LabelFontSize.small1.size,
                    color: color.text.tertiary,
                  ),
                ),
              ],
            ),
          ),

        if (inReplyTo case (_, final inReplyToMessage))
          Padding(
            padding: const EdgeInsets.only(top: Spacings.xs),
            child: Stack(
              children: [
                Padding(
                  padding: const EdgeInsets.only(
                    top: Spacings.xxxs,
                    right: Spacings.xxxs,
                  ),
                  child: InReplyToBubble(
                    inReplyTo: inReplyToMessage,
                    backgroundColor: color.fill.secondary,
                    stretch: true,
                  ),
                ),
                Positioned(
                  top: 0,
                  right: 0,
                  child: Container(
                    decoration: BoxDecoration(
                      color: color.backgroundElevated.primary,
                      shape: BoxShape.circle,
                    ),
                    constraints: BoxConstraints.tight(const Size.square(20)),
                    child: IconButton(
                      icon: const AppIcon.x(size: 12),
                      constraints: const BoxConstraints(
                        minHeight: Spacings.xxs,
                        minWidth: Spacings.xxs,
                      ),
                      padding: EdgeInsets.zero,
                      onPressed: () {
                        context.read<ChatDetailsCubit>().resetDraftReply();
                      },
                    ),
                  ),
                ),
              ],
            ),
          ),
        CompositedTransformTarget(
          key: inputKey,
          link: layerLink,
          child: TextField(
            focusNode: _focusNode,
            controller: _controller,
            style: TextStyle(
              fontSize: _composerFontSize,
              height: _composerLineHeight,
              leadingDistribution: TextLeadingDistribution.even,
              color: color.text.primary,
            ),
            minLines: 1,
            maxLines: 10,
            textAlignVertical: TextAlignVertical.center,
            enabled: isConfirmedChat,
            decoration: InputDecoration(
              isCollapsed: true,
              contentPadding: const EdgeInsets.symmetric(
                vertical: _inputVerticalPadding,
              ),
              hintText: loc.composer_inputHint(chatTitle ?? ""),
              hintMaxLines: 1,
              hintStyle: TextStyle(
                color: color.text.tertiary,
                overflow: TextOverflow.ellipsis,
              ),
            ).copyWith(filled: false),
            contextMenuBuilder: _contextMenuBuilder,
            textInputAction: sendOnEnter
                ? TextInputAction.send
                : TextInputAction.newline,
            onEditingComplete: sendOnEnter
                ? onSubmitMessage
                : () => _focusNode.requestFocus(),
            keyboardType: TextInputType.multiline,
            textCapitalization: TextCapitalization.sentences,
          ),
        ),
      ],
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
            final imageBytes = await getClipboardImage();
            if (imageBytes != null && imageBytes.isNotEmpty) {
              editableTextState.hideToolbar();
              onImagePasted(imageBytes);
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
            final imageBytes = await getClipboardImage();
            if (imageBytes != null && imageBytes.isNotEmpty) {
              editableTextState.hideToolbar();
              onImagePasted(imageBytes);
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

/// Builds the path for the composer element shapes (circles for buttons,
/// rounded rect for the input field), optionally inflated and offset for
/// shadow rendering.
Path _buildComposerPath(
  Size size, {
  required double buttonSize,
  required double spacing,
  required double inputBorderRadius,
  required int trailingButtonCount,
  double inflate = 0,
  Offset offset = Offset.zero,
}) {
  final path = Path();
  double x = 0;
  final bottom = size.height;
  final buttonTop = bottom - buttonSize;

  // Leading button (action) — circle aligned to bottom.
  path.addOval(
    Rect.fromLTWH(
      x,
      buttonTop,
      buttonSize,
      buttonSize,
    ).inflate(inflate).shift(offset),
  );
  x += buttonSize + spacing;

  // Input field — takes remaining width.
  final trailingWidth = trailingButtonCount * (spacing + buttonSize);
  final inputWidth = size.width - x - trailingWidth;
  path.addRRect(
    RRect.fromRectAndRadius(
      Rect.fromLTWH(
        x,
        0,
        inputWidth,
        size.height,
      ).inflate(inflate).shift(offset),
      Radius.circular(inputBorderRadius + inflate),
    ),
  );
  x += inputWidth;

  // Trailing buttons — circles aligned to bottom.
  for (int i = 0; i < trailingButtonCount; i++) {
    x += spacing;
    path.addOval(
      Rect.fromLTWH(
        x,
        buttonTop,
        buttonSize,
        buttonSize,
      ).inflate(inflate).shift(offset),
    );
    x += buttonSize;
  }

  return path;
}

/// Clips to the union of the composer's element shapes so a single
/// [BackdropFilter] can blur only behind those elements.
class _ComposerClipper extends CustomClipper<Path> {
  const _ComposerClipper({
    required this.buttonSize,
    required this.spacing,
    required this.inputBorderRadius,
    required this.trailingButtonCount,
  });

  final double buttonSize;
  final double spacing;
  final double inputBorderRadius;
  final int trailingButtonCount;

  @override
  Path getClip(Size size) {
    return _buildComposerPath(
      size,
      buttonSize: buttonSize,
      spacing: spacing,
      inputBorderRadius: inputBorderRadius,
      trailingButtonCount: trailingButtonCount,
    );
  }

  @override
  bool shouldReclip(covariant _ComposerClipper old) {
    return buttonSize != old.buttonSize ||
        spacing != old.spacing ||
        inputBorderRadius != old.inputBorderRadius ||
        trailingButtonCount != old.trailingButtonCount;
  }
}

/// Paints the composer element shadows, clipped to the exterior of the
/// element shapes so shadow doesn't bleed through the semi-transparent fills.
class _ComposerShadowPainter extends CustomPainter {
  const _ComposerShadowPainter({
    required this.buttonSize,
    required this.spacing,
    required this.inputBorderRadius,
    required this.trailingButtonCount,
  });

  final double buttonSize;
  final double spacing;
  final double inputBorderRadius;
  final int trailingButtonCount;

  @override
  void paint(Canvas canvas, Size size) {
    final interiorPath = _buildComposerPath(
      size,
      buttonSize: buttonSize,
      spacing: spacing,
      inputBorderRadius: inputBorderRadius,
      trailingButtonCount: trailingButtonCount,
    );

    final outerPath = Path()..addRect((Offset.zero & size).inflate(100));
    canvas.save();
    canvas.clipPath(
      Path.combine(PathOperation.difference, outerPath, interiorPath),
    );

    for (final shadow in largeElevationBoxShadows) {
      canvas.drawPath(
        _buildComposerPath(
          size,
          buttonSize: buttonSize,
          spacing: spacing,
          inputBorderRadius: inputBorderRadius,
          trailingButtonCount: trailingButtonCount,
          inflate: shadow.spreadRadius,
          offset: shadow.offset,
        ),
        shadow.toPaint(),
      );
    }

    canvas.restore();
  }

  @override
  bool shouldRepaint(covariant _ComposerShadowPainter old) {
    return buttonSize != old.buttonSize ||
        spacing != old.spacing ||
        inputBorderRadius != old.inputBorderRadius ||
        trailingButtonCount != old.trailingButtonCount;
  }
}

enum Direction { right, left }

class InReplyToBubble extends StatelessWidget {
  const InReplyToBubble({
    super.key,
    required this.inReplyTo,
    this.backgroundColor,
    this.stretch = false,
  });

  final UiInReplyToMessage inReplyTo;
  final Color? backgroundColor;
  final bool stretch;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final color = CustomColorScheme.of(context);

    // there are a few reasons why a message can't be resolved, for example:
    // a message was deleted locally (only for me) or you joined a group
    // and later somebody sent a message referencing a message you don't have
    // access to. Unfortunately those two cases can't be differentiated.
    final (senderDisplayName, contentPreview) = switch (inReplyTo) {
      UiInReplyToMessage_NotFound() => (
        loc.composer_reply_noaccess_message_user,
        loc.composer_reply_noaccess_message_placeholder,
      ),
      UiInReplyToMessage_Resolved(:final sender, :final mimiContent) => (
        mimiContent.isDeleted
            ? null
            : context.select(
                (UsersCubit cubit) => cubit.state.displayName(userId: sender),
              ),
        mimiContent.plaintextPreview(loc) ??
            loc.composer_reply_deleted_message_placeholder,
      ),
    };
    // Show the jump arrow only in message bubbles when the original message
    // exists and hasn't been deleted. Don't show the arrow in the compose
    // preview.
    final showJumpIcon =
        !stretch &&
        inReplyTo is UiInReplyToMessage_Resolved &&
        !(inReplyTo as UiInReplyToMessage_Resolved).mimiContent.isDeleted;

    final innerContent = Container(
      decoration: BoxDecoration(
        border: Border(
          left: BorderSide(color: color.separator.primary, width: 1),
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
        child: Column(
          crossAxisAlignment: stretch ? .stretch : .start,
          children: [
            if (senderDisplayName != null)
              Text(
                senderDisplayName,
                style: TextStyle(
                  fontSize: LabelFontSize.small1.size,
                  fontWeight: FontWeight.bold,
                  color: color.text.primary,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            Text(
              contentPreview,
              style: TextStyle(
                fontSize: LabelFontSize.small1.size,
                color: color.text.secondary,
              ),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
      ),
    );

    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.xs,
        vertical: Spacings.xxs,
      ),
      decoration: BoxDecoration(
        borderRadius: const BorderRadius.all(Radius.circular(Spacings.xxs)),
        color: backgroundColor,
      ),
      child: showJumpIcon
          ? Stack(
              children: [
                innerContent,
                PositionedDirectional(
                  top: 0,
                  end: 0,
                  child: AppIcon.arrowUp(size: 12, color: color.text.tertiary),
                ),
              ],
            )
          : innerContent,
    );
  }
}
