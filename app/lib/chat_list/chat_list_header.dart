// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:logging/logging.dart';
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
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:provider/provider.dart';

final _log = Logger("ChatListHeader");

class ChatListHeader extends StatelessWidget {
  const ChatListHeader({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.only(
        left: Spacings.xxs,
        right: Spacings.s,
        bottom: Spacings.xs,
      ),
      child: const Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [_Avatar(), _PlusButton()],
      ),
    );
  }
}

class _Avatar extends StatefulWidget {
  const _Avatar();

  @override
  State<_Avatar> createState() => _AvatarState();
}

class _AvatarState extends State<_Avatar> {
  final contextMenuController = OverlayPortalController();

  @override
  Widget build(BuildContext context) {
    final profile = context.select((UsersCubit cubit) => cubit.state.profile());

    return Padding(
      padding: const EdgeInsets.only(left: Spacings.sm),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          GestureDetector(
            onTap: () {
              context.read<NavigationCubit>().openUserSettings();
            },
            onLongPress: () {
              context.read<NavigationCubit>().openDeveloperSettings();
            },
            child: UserAvatar(
              displayName: profile.displayName,
              image: profile.profilePicture,
              size: Spacings.l,
            ),
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

    return ContextMenu(
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
            child: Icon(
              Icons.add_rounded,
              size: 22,
              color: CustomColorScheme.of(context).text.primary,
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
      final plaintext = value?.trim().toLowerCase();
      if (plaintext == null || plaintext.isEmpty) {
        return loc.newConnectionDialog_error_emptyHandle;
      }
      if (customError != null) {
        final error = customError;
        customError = null;
        return error;
      }
      UiUserHandle handle = UiUserHandle(plaintext: plaintext);
      return handle.validationError();
    }

    Future<String?> onAction(String input) async {
      final handle = UiUserHandle(plaintext: input.trim().toLowerCase());
      try {
        final chatId = await chatListCubit.createContactChat(handle: handle);
        if (context.mounted) {
          if (chatId == null) {
            return loc.newConnectionDialog_error_handleNotFound(
              handle.plaintext,
            );
          }
          _log.info(
            "A new 1:1 connection with user '${handle.plaintext}' was created: "
            "chatId = $chatId",
          );
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
      builder:
          (BuildContext context) => CreateChatView(
            context,
            loc.newConnectionDialog_newConnectionTitle,
            loc.newConnectionDialog_newConnectionDescription,
            loc.newConnectionDialog_usernamePlaceholder,
            loc.newConnectionDialog_actionButton,
            validator: validator,
            onAction: onAction,
          ),
    );
  }

  void _newGroup(BuildContext context) async {
    final chatListCubit = context.read<ChatListCubit>();
    final loc = AppLocalizations.of(context);

    validator(String? value) {
      final plaintext = value?.trim().toLowerCase();
      if (plaintext == null || plaintext.isEmpty) {
        return loc.newChatDialog_error_emptyGroupName;
      }
      return null;
    }

    String? input = await showDialog(
      context: context,
      builder:
          (BuildContext context) => CreateChatView(
            context,
            loc.newChatDialog_newChatTitle,
            loc.newChatDialog_newChatDescription,
            loc.newChatDialog_chatNamePlaceholder,
            loc.newChatDialog_actionButton,
            validator: validator,
          ),
    );
    String groupName = input?.trim() ?? "";
    if (groupName.isNotEmpty) {
      try {
        final chatId = await chatListCubit.createGroupChat(
          groupName: groupName,
        );
        _log.info(
          "A new group '$groupName' was created: "
          "chatId = $chatId",
        );
      } catch (e) {
        if (context.mounted) {
          _log.severe("Failed to create chat: $e");
          showErrorBanner(context, loc.newChatDialog_error(groupName));
        }
      }
    }
  }
}
