// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/attachments/attachments.dart';
import 'package:air/l10n/app_localizations_extension.dart';
import 'package:air/main.dart';
import 'package:air/message_list/emoji_repository.dart';
import 'package:air/message_list/emoji_autocomplete.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/user/user_settings_cubit.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/message_list/widgets/text_autocomplete.dart';
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
import 'package:air/ui/typography/font_size.dart';
import 'package:provider/provider.dart';

import 'package:air/util/platform.dart' show getClipboardImage;

import 'message_renderer.dart';

final _log = Logger("MessageComposer");
const double _composerLineHeight = 1.3;
final double _composerFontSize = BodyFontSize.base.size;

class MessageComposer extends StatefulWidget {
  const MessageComposer({super.key});

  @override
  State<MessageComposer> createState() => _MessageComposerState();
}

class _MessageComposerState extends State<MessageComposer>
    with WidgetsBindingObserver, TickerProviderStateMixin {
  final TextEditingController _inputController = CustomTextEditingController();
  final Debouncer _storeDraftDebouncer = Debouncer(
    delay: const Duration(milliseconds: 300),
  );
  StreamSubscription<ChatDetailsState>? _draftLoadingSubscription;
  final _focusNode = FocusNode();
  late ChatDetailsCubit _chatDetailsCubit;
  bool _inputIsEmpty = true;
  final LayerLink _inputFieldLink = LayerLink();
  final GlobalKey _inputFieldKey = GlobalKey();
  late final TextAutocompleteController<EmojiEntry> _emojiAutocomplete;
  double _actionButtonSize = _defaultActionButtonSize;
  bool _actionButtonSizeUpdateScheduled = false;

  static const double _defaultActionButtonSize = 48;
  static const double _maxActionButtonSize = Spacings.xl;

  @override
  void initState() {
    super.initState();
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

    // Propagate loaded draft to the text field.
    //
    // There are two cases when the changes are propagated:
    //
    // 1. Initially loaded draft
    // 2. Editing ID has changed
    MessageId? currentEditingId;
    _draftLoadingSubscription = _chatDetailsCubit.stream.listen((state) {
      // Check that chat is fully loaded
      if (state.chat == null) {
        return;
      }
      switch (state.chat?.draft) {
        // Initially loaded draft
        case final draft? when draft.isCommitted:
          // If input controller is not empty, then the user already typed something,
          // and we don't want to overwrite it.
          if (_inputController.text.isEmpty) {
            _inputController.text = draft.message;
          }
        // Editing ID has changed
        case final draft when draft?.editingId != currentEditingId:
          // If input controller is not empty, then the user already typed something,
          // and we don't want to overwrite it.
          if (_inputController.text.isEmpty) {
            _inputController.text = draft?.message ?? "";
          }
          currentEditingId = draft?.editingId;
        default:
      }
    });

    _scheduleActionButtonSizeUpdate();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _scheduleActionButtonSizeUpdate();
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
    final (chatTitle, editingId, isConfirmedChat) = context.select((
      ChatDetailsCubit cubit,
    ) {
      final chat = cubit.state.chat;
      return (chat?.title, chat?.draft?.editingId, chat?.isConfirmed ?? false);
    });

    if (chatTitle == null) {
      return const SizedBox.shrink();
    }

    _scheduleActionButtonSizeUpdate();

    return AnimatedContainer(
      duration: const Duration(milliseconds: 1000),
      child: Container(
        color: CustomColorScheme.of(context).backgroundBase.primary,
        padding: const EdgeInsets.only(
          top: Spacings.xs,
          left: Spacings.xs,
          right: Spacings.xs,
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Expanded(
              child: Container(
                decoration: BoxDecoration(
                  color: CustomColorScheme.of(context).backgroundBase.secondary,
                  borderRadius: BorderRadius.circular(Spacings.m),
                ),
                padding: const EdgeInsets.only(
                  left: Spacings.xs,
                  right: Spacings.xs,
                ),
                child: _MessageInput(
                  focusNode: _focusNode,
                  controller: _inputController,
                  chatTitle: chatTitle,
                  isEditing: editingId != null,
                  layerLink: _inputFieldLink,
                  inputKey: _inputFieldKey,
                  onSubmitMessage: () =>
                      _submitMessage(context.read<ChatDetailsCubit>()),
                  onImagePasted: _handleImagePaste,
                ),
              ),
            ),
            if (editingId != null)
              Container(
                width: _actionButtonSize,
                height: _actionButtonSize,
                margin: const EdgeInsets.only(left: Spacings.xs),
                decoration: BoxDecoration(
                  color: CustomColorScheme.of(context).backgroundBase.secondary,
                  borderRadius: BorderRadius.circular(_maxActionButtonSize),
                ),
                child: IconButton(
                  icon: AppIcon.x(size: _actionButtonSize / 2),
                  color: CustomColorScheme.of(context).text.primary,
                  hoverColor: const Color(0x00FFFFFF),
                  onPressed: () {
                    context.read<ChatDetailsCubit>().resetDraft();
                    _inputController.clear();
                  },
                ),
              ),
            Container(
              width: _actionButtonSize,
              height: _actionButtonSize,
              margin: const EdgeInsets.only(left: Spacings.xs),
              decoration: BoxDecoration(
                color: CustomColorScheme.of(context).backgroundBase.secondary,
                borderRadius: BorderRadius.circular(_maxActionButtonSize),
              ),
              child: IconButton(
                icon: _inputIsEmpty
                    ? AppIcon.plus(size: _actionButtonSize / 2)
                    : AppIcon.arrowUp(size: _actionButtonSize / 2),
                color: CustomColorScheme.of(context).text.primary,
                hoverColor: const Color(0x00FFFFFF),
                onPressed: isConfirmedChat
                    ? () {
                        if (_inputIsEmpty) {
                          _uploadAttachment(context, chatTitle: chatTitle);
                        } else {
                          _submitMessage(context.read());
                        }
                      }
                    : null,
              ),
            ),
          ],
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
    } else {
      return KeyEventResult.ignored;
    }
  }

  void _submitMessage(ChatDetailsCubit chatDetailsCubit) async {
    final messageText = _inputController.text.trim();
    if (messageText.isEmpty) {
      return;
    }

    // FIXME: Handle errors
    if (messageText == "delete") {
      chatDetailsCubit.deleteMessage();
    } else {
      chatDetailsCubit.sendMessage(messageText);
    }

    setState(() {
      _inputController.clear();
      _focusNode.requestFocus();
    });
  }

  bool _editMessage(ChatDetailsCubit cubit) {
    if (_inputController.text.trim().isNotEmpty) {
      return false;
    }
    if (cubit.state.chat?.draft?.editingId != null) {
      return false;
    }
    cubit.editMessage();
    return true;
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

    // Reuce image quality to re-encode the image.
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

  void _handleImagePaste(Uint8List imageBytes) async {
    final chatTitle = _chatDetailsCubit.state.chat?.title;
    if (chatTitle == null) return;

    final tempDir = await getTemporaryDirectory();
    final tempFile = File(
      '${tempDir.path}/paste_${DateTime.now().millisecondsSinceEpoch}.png',
    );
    await tempFile.writeAsBytes(imageBytes);
    final file = XFile(tempFile.path);

    if (!mounted) return;
    _navigateToUploadPreview(context, file, chatTitle: chatTitle);
  }

  void _navigateToUploadPreview(
    BuildContext context,
    XFile file, {
    required String chatTitle,
  }) {
    final cubit = context.read<ChatDetailsCubit>();

    Navigator.of(context).push(
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
              if (!context.mounted) return;
              final loc = AppLocalizations.of(context);
              showErrorBanner(context, loc.composer_error_attachment);
            }
          },
        ),
      ),
    );
  }

  void _onTextChanged() {
    setState(() {
      _inputIsEmpty = _inputController.text.trim().isEmpty;
    });
    _storeDraftDebouncer.run(() {
      _chatDetailsCubit.storeDraft(
        draftMessage: _inputController.text,
        isCommitted: false,
      );
    });
    _emojiAutocomplete.handleTextChanged();
    _scheduleActionButtonSizeUpdate();
  }

  void _scheduleActionButtonSizeUpdate() {
    if (_actionButtonSizeUpdateScheduled) {
      return;
    }
    _actionButtonSizeUpdateScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _actionButtonSizeUpdateScheduled = false;
      if (!mounted) {
        return;
      }
      _updateActionButtonSize();
    });
  }

  void _updateActionButtonSize() {
    final newHeight = _inputFieldKey.currentContext?.size?.height;
    if (newHeight == null || newHeight <= 0) {
      return;
    }
    final targetHeight = newHeight.clamp(0.0, _maxActionButtonSize).toDouble();
    if ((_actionButtonSize - targetHeight).abs() < 0.5) {
      return;
    }
    setState(() {
      _actionButtonSize = targetHeight;
    });
  }
}

class _MessageInput extends StatelessWidget {
  const _MessageInput({
    required FocusNode focusNode,
    required TextEditingController controller,
    required this.chatTitle,
    required this.isEditing,
    required this.layerLink,
    required this.inputKey,
    required this.onSubmitMessage,
    required this.onImagePasted,
  }) : _focusNode = focusNode,
       _controller = controller;

  final FocusNode _focusNode;
  final TextEditingController _controller;
  final String? chatTitle;
  final bool isEditing;
  final LayerLink layerLink;
  final GlobalKey inputKey;
  final VoidCallback onSubmitMessage;
  final ValueChanged<Uint8List> onImagePasted;

  @override
  Widget build(BuildContext context) {
    final sendOnEnter = context.select(
      (UserSettingsCubit cubit) => cubit.state.sendOnEnter,
    );

    final isConfirmedChat = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isConfirmed ?? false,
    );

    final loc = AppLocalizations.of(context);
    final color = CustomColorScheme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
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
        CompositedTransformTarget(
          key: inputKey,
          link: layerLink,
          child: TextField(
            focusNode: _focusNode,
            controller: _controller,
            style: TextStyle(
              fontSize: _composerFontSize,
              height: _composerLineHeight,
              color: color.text.primary,
            ),
            minLines: 1,
            maxLines: 10,
            enabled: isConfirmedChat,
            decoration: InputDecoration(
              isDense: true,
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
    // button on Android & iOS. Add one so the user can paste images.
    if (!hasPaste && (Platform.isIOS || Platform.isAndroid)) {
      items.add(
        ContextMenuButtonItem(
          type: ContextMenuButtonType.paste,
          onPressed: () async {
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

enum Direction { right, left }
