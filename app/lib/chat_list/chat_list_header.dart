// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/chat_list/add_contact_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/context_menu/context_menu.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

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
    showDialog(
      context: context,
      builder: (BuildContext context) => BlocProvider.value(
        value: chatListCubit,
        child: const AddContactDialog(),
      ),
    );
  }

  void _newGroup(BuildContext context) {
    context.read<NavigationCubit>().openCreateGroup();
  }
}
