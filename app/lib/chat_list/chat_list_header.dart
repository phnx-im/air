// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/chat_list/create_chat_view.dart';
import 'package:air/core/api/types.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/context_menu/context_menu.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

final _log = Logger("ChatListHeader");

class ChatListHeader extends StatelessWidget {
  const ChatListHeader({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.only(left: Spacings.xxs, right: Spacings.xxs),
      child: const Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [_Avatar(), _PlusButton()],
      ),
    );
  }
}

class _Avatar extends StatelessWidget {
  const _Avatar();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(left: Spacings.sm),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          GestureDetector(
            onTap: () {
              context.read<NavigationCubit>().openUserProfile();
            },
            onLongPress: () {
              context.read<NavigationCubit>().openDeveloperSettings();
            },
            child: const UserAvatar(size: Spacings.l),
          ),
        ],
      ),
    );
  }
}

class _PlusButton extends StatefulWidget {
  const _PlusButton();

  @override
  State<_PlusButton> createState() => _PlusButtonState();
}

class _PlusButtonState extends State<_PlusButton> {
  final contextMenuController = OverlayPortalController();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.xs,
        vertical: Spacings.xs,
      ),
      child: ContextMenu(
        direction: ContextMenuDirection.left,
        width: 200,
        controller: contextMenuController,
        menuItems: [
          ContextMenuItem(
            label: loc.chatList_newContact,
            onPressed: () {
              _newContact(context);
            },
          ),
          ContextMenuItem(
            label: loc.chatList_newGroup,
            onPressed: () {
              _newGroup(context);
            },
          ),
        ],
        child: TextButton(
          style: TextButton.styleFrom(
            padding: EdgeInsets.zero,
            minimumSize: Size.zero,
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          ),
          onPressed: () {
            contextMenuController.show();
          },
          child: Container(
            width: 32,
            height: 32,
            decoration: BoxDecoration(
              color: CustomColorScheme.of(context).backgroundBase.quaternary,
              borderRadius: BorderRadius.circular(16),
            ),
            child: Center(
              child: iconoir.Plus(
                color: CustomColorScheme.of(context).text.primary,
                width: 22,
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _newContact(BuildContext context) {
    final chatListCubit = context.read<ChatListCubit>();
    final loc = AppLocalizations.of(context);

    String? customError;

    String? validator(String? value) {
      final normalized = UserHandleInputFormatter.normalize(
        value ?? '',
        allowUnderscore: true,
      );
      if (normalized.isEmpty) {
        return loc.newConnectionDialog_error_emptyHandle;
      }
      if (customError != null) {
        final error = customError;
        customError = null;
        return error;
      }
      UiUserHandle handle = UiUserHandle(plaintext: normalized);
      return handle.validationError();
    }

    Future<String?> onAction(String input) async {
      final normalized = UserHandleInputFormatter.normalize(
        input,
        allowUnderscore: true,
      );
      if (normalized.isEmpty) {
        return loc.newConnectionDialog_error_emptyHandle;
      }
      final handle = UiUserHandle(plaintext: normalized);
      try {
        final res = await chatListCubit.createContactChat(handle: handle);
        if (context.mounted) {
          switch (res) {
            case AddHandleContactResult_Err(field0: final err):
              switch (err) {
                case AddHandleContactError.handleNotFound:
                  return loc.newConnectionDialog_error_handleNotFound(
                    handle.plaintext,
                  );
                case AddHandleContactError.duplicateRequest:
                  return loc.newConnectionDialog_error_duplicateRequest;
                case AddHandleContactError.ownHandle:
                  return loc.newConnectionDialog_error_ownHandle;
              }
            case AddHandleContactResult_Ok(field0: final chatId):
              _log.info(
                "A new 1:1 connection with user '${handle.plaintext}' was created: "
                "chatId = $chatId",
              );
          }
          Navigator.of(context).pop();
        }
      } catch (e) {
        _log.severe("Failed to create connection: $e");
        if (context.mounted) {
          showErrorBanner(
            context,
            loc.newConnectionDialog_error(handle.plaintext),
          );
        }
      }
      return null;
    }

    showDialog(
      context: context,
      builder: (BuildContext context) => CreateChatView(
        context,
        loc.newConnectionDialog_newConnectionTitle,
        loc.newConnectionDialog_newConnectionDescription,
        loc.newConnectionDialog_usernamePlaceholder,
        loc.newConnectionDialog_actionButton,
        validator: validator,
        onAction: onAction,
        allowUnderscore: true,
      ),
    );
  }

  void _newGroup(BuildContext context) {
    context.read<NavigationCubit>().openCreateGroup();
  }
}
