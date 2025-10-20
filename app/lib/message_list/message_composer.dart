// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/user/user_settings_cubit.dart';
import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/regular/edit_pencil.dart';
import 'package:iconoir_flutter/regular/plus.dart';
import 'package:iconoir_flutter/regular/send.dart';
import 'package:iconoir_flutter/regular/xmark.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';
import 'package:air/chat_details/chat_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/main.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/util/debouncer.dart';
import 'package:provider/provider.dart';

import 'message_renderer.dart';

final _log = Logger("MessageComposer");

class MessageComposer extends StatefulWidget {
  const MessageComposer({super.key});

  @override
  State<MessageComposer> createState() => _MessageComposerState();
}

class _MessageComposerState extends State<MessageComposer>
    with WidgetsBindingObserver {
  final TextEditingController _inputController = CustomTextEditingController();
  final Debouncer _storeDraftDebouncer = Debouncer(
    delay: const Duration(milliseconds: 500),
  );
  StreamSubscription<ChatDetailsState>? _draftLoadingSubscription;
  final _focusNode = FocusNode();
  late ChatDetailsCubit _chatDetailsCubit;
  bool _keyboardVisible = false;
  bool _inputIsEmpty = true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _focusNode.onKeyEvent = _onKeyEvent;
    _inputController.addListener(_onTextChanged);

    _chatDetailsCubit = context.read<ChatDetailsCubit>();

    // Propagate draft changes to the text field.
    // In particular, this sets the draft message on initial load, if any.

    _draftLoadingSubscription = _chatDetailsCubit.stream.listen((state) {
      if (state.chat != null) {
        // state is fully loaded
        if (state.chat?.draft case final draft?) {
          // We have a draft
          // Ignore user drafts, those were input here and just reflect the change state.

          switch (draft.source) {
            case UiMessageDraftSource.system:
              // If input controller is not empty, then the user already typed something,
              // and we don't want to overwrite it.
              if (_inputController.text.isEmpty) {
                _inputController.text = draft.message;
              }
              break;
            case UiMessageDraftSource.user:
              // Ingore user drafts; they just reflect the past state of the input controller.
              break;
          }
        }
      }
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _storeDraftDebouncer.dispose();

    _chatDetailsCubit.storeDraft(draftMessage: _inputController.text);
    _inputController.dispose();

    _draftLoadingSubscription?.cancel();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  void didChangeMetrics() {
    final view = View.of(context);
    final bottomInset = view.viewInsets.bottom;
    final keyboardVisible = bottomInset > 0.0;

    if (_keyboardVisible != keyboardVisible) {
      setState(() {
        _keyboardVisible = keyboardVisible;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final (chatTitle, editingId) = context.select(
      (ChatDetailsCubit cubit) => (
        cubit.state.chat?.title,
        cubit.state.chat?.draft?.editingId,
      ),
    );

    if (chatTitle == null) {
      return const SizedBox.shrink();
    }

    return AnimatedContainer(
      duration: const Duration(milliseconds: 1000),
      child: Container(
        color: CustomColorScheme.of(context).backgroundBase.primary,
        padding: EdgeInsets.only(
          top: Spacings.xs,
          bottom:
              isSmallScreen(context) && !_keyboardVisible
                  ? Spacings.m
                  : Spacings.xs,
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
                icon:
                    _inputIsEmpty
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
                    _uploadAttachment(context);
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

  void _uploadAttachment(BuildContext context) async {
    final ImagePicker picker = ImagePicker();
    final XFile? file = await picker.pickImage(source: ImageSource.gallery);

    if (file == null) {
      return;
    }

    if (!context.mounted) {
      return;
    }

    final cubit = context.read<ChatDetailsCubit>();
    try {
      await cubit.uploadAttachment(file.path);
    } catch (e) {
      _log.severe("Failed to upload attachment: $e");
      if (context.mounted) {
        final loc = AppLocalizations.of(context);
        showErrorBanner(context, loc.composer_error_attachment);
      }
    }
  }

  void _onTextChanged() {
    setState(() {
      _inputIsEmpty = _inputController.text.trim().isEmpty;
    });
    _storeDraftDebouncer.run(() {
      _chatDetailsCubit.storeDraft(draftMessage: _inputController.text);
    });
  }
}

class _MessageInput extends StatelessWidget {
  const _MessageInput({
    required FocusNode focusNode,
    required TextEditingController controller,
    required this.chatTitle,
    required this.isEditing,
  }) : _focusNode = focusNode,
       _controller = controller;

  final FocusNode _focusNode;
  final TextEditingController _controller;
  final String? chatTitle;
  final bool isEditing;

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
        TextField(
          focusNode: _focusNode,
          controller: _controller,
          minLines: 1,
          maxLines: 10,
          decoration: InputDecoration(
            hintText: loc.composer_inputHint(chatTitle ?? ""),
            hintStyle: TextStyle(
              color: CustomColorScheme.of(context).text.tertiary,
            ),
          ).copyWith(filled: false),
          textInputAction:
              sendOnEnter ? TextInputAction.send : TextInputAction.newline,
          onEditingComplete: () => _focusNode.requestFocus(),
          keyboardType: TextInputType.multiline,
          textCapitalization: TextCapitalization.sentences,
        ),
      ],
    );
  }
}

enum Direction { right, left }
