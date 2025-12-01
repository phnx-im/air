// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/attachments/attachments.dart';
import 'package:air/main.dart';
import 'package:air/message_list/emoji_repository.dart';
import 'package:air/message_list/emoji_autocomplete.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:air/user/user_settings_cubit.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/message_list/widgets/text_autocomplete.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:iconoir_flutter/regular/edit_pencil.dart';
import 'package:iconoir_flutter/regular/plus.dart';
import 'package:iconoir_flutter/regular/send.dart';
import 'package:iconoir_flutter/regular/xmark.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:provider/provider.dart';

import 'message_renderer.dart';

final _log = Logger("MessageComposer");

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
    final (chatTitle, editingId) = context.select(
      (ChatDetailsCubit cubit) =>
          (cubit.state.chat?.title, cubit.state.chat?.draft?.editingId),
    );

    if (chatTitle == null) {
      return const SizedBox.shrink();
    }

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
          crossAxisAlignment: CrossAxisAlignment.center,
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
                ),
              ),
            ),
            if (editingId != null)
              Container(
                width: 50,
                height: 50,
                margin: const EdgeInsets.only(left: Spacings.xs),
                decoration: BoxDecoration(
                  color: CustomColorScheme.of(context).backgroundBase.secondary,
                  borderRadius: BorderRadius.circular(Spacings.m),
                ),
                child: IconButton(
                  icon: Xmark(
                    color: CustomColorScheme.of(context).text.primary,
                    width: 32,
                  ),
                  color: CustomColorScheme.of(context).text.primary,
                  hoverColor: const Color(0x00FFFFFF),
                  onPressed: () {
                    context.read<ChatDetailsCubit>().resetDraft();
                    _inputController.clear();
                  },
                ),
              ),
            Container(
              width: 50,
              height: 50,
              margin: const EdgeInsets.only(left: Spacings.xs),
              decoration: BoxDecoration(
                color: CustomColorScheme.of(context).backgroundBase.secondary,
                borderRadius: BorderRadius.circular(Spacings.m),
              ),
              child: IconButton(
                icon: _inputIsEmpty
                    ? Plus(
                        color: CustomColorScheme.of(context).text.primary,
                        width: 32,
                      )
                    : Send(
                        color: CustomColorScheme.of(context).text.primary,
                        width: 32,
                      ),
                color: CustomColorScheme.of(context).text.primary,
                hoverColor: const Color(0x00FFFFFF),
                onPressed: () {
                  if (_inputIsEmpty) {
                    _uploadAttachment(context, chatTitle: chatTitle);
                  } else {
                    _submitMessage(context.read());
                  }
                },
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

    final XFile? file = switch (selectedCategory) {
      .gallery => await ImagePicker().pickImage(source: .gallery),
      .camera => await ImagePicker().pickImage(source: .camera),
      .file => await openFile(),
      null => null,
    };

    if (file == null) {
      return;
    }

    if (!context.mounted) {
      return;
    }

    final cubit = context.read<ChatDetailsCubit>();

    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => AttachmentUploadView(
          title: chatTitle,
          file: file,
          onUpload: () async {
            try {
              await cubit.uploadAttachment(file.path);
            } catch (e) {
              _log.severe("Failed to upload attachment: $e");
              if (context.mounted) {
                final loc = AppLocalizations.of(context);
                showErrorBanner(context, loc.composer_error_attachment);
              }
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
  }) : _focusNode = focusNode,
       _controller = controller;

  final FocusNode _focusNode;
  final TextEditingController _controller;
  final String? chatTitle;
  final bool isEditing;
  final LayerLink layerLink;
  final GlobalKey inputKey;

  @override
  Widget build(BuildContext context) {
    final sendOnEnter = context.select(
      (UserSettingsCubit cubit) => cubit.state.sendOnEnter,
    );

    final loc = AppLocalizations.of(context);

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
                EditPencil(color: CustomColorScheme.of(context).text.tertiary),
                const SizedBox(width: Spacings.xxs),
                Text(
                  loc.composer_editMessage,
                  style: TextStyle(
                    fontSize: LabelFontSize.small1.size,
                    color: CustomColorScheme.of(context).text.tertiary,
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
            minLines: 1,
            maxLines: 10,
            decoration: InputDecoration(
              hintText: loc.composer_inputHint(chatTitle ?? ""),
              hintMaxLines: 1,
              hintStyle: TextStyle(
                color: CustomColorScheme.of(context).text.tertiary,
                overflow: TextOverflow.ellipsis,
              ),
            ).copyWith(filled: false),
            textInputAction: sendOnEnter
                ? TextInputAction.send
                : TextInputAction.newline,
            onEditingComplete: () => _focusNode.requestFocus(),
            keyboardType: TextInputType.multiline,
            textCapitalization: TextCapitalization.sentences,
          ),
        ),
      ],
    );
  }
}

enum Direction { right, left }
