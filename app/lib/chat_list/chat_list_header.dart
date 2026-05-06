// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat_list/chat_list_cubit.dart';
import 'package:air/chat_list/add_contact_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/components/button/glass_circle_button.dart';
import 'package:air/ui/icons/app_icons.dart';
import 'package:air/ui/components/context_menu/context_menu.dart';
import 'package:air/ui/components/context_menu/context_menu_item_ui.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Width reserved for each slot of the header (plus button / avatar).
const double _kHeaderSlotSize = 40;

class ChatListHeader extends StatelessWidget {
  const ChatListHeader({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final isMobile = isSmallScreen(context);

    return Padding(
      padding: const EdgeInsets.only(left: Spacings.sm, right: Spacings.s),
      child: SizedBox(
        height: kToolbarHeight,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            const SizedBox(
              width: _kHeaderSlotSize,
              child: Center(child: _PlusButton()),
            ),
            Expanded(
              child: Center(
                child: isMobile
                    ? Text(
                        loc.homeTab_chats,
                        style: TextStyle(
                          fontSize: LabelFontSize.base.size,
                          fontWeight: FontWeight.bold,
                        ),
                      )
                    : null,
              ),
            ),
            SizedBox(
              width: _kHeaderSlotSize,
              child: isMobile ? null : const Center(child: _Avatar()),
            ),
          ],
        ),
      ),
    );
  }
}

class _Avatar extends StatelessWidget {
  const _Avatar();

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: () {
        context.read<NavigationCubit>().switchTab(HomeTab.profile);
      },
      onLongPress: () {
        context.read<NavigationCubit>().openDeveloperSettings();
      },
      child: Builder(
        builder: (context) {
          final profile = context.select(
            (UsersCubit cubit) => cubit.state.profile(userId: null),
          );
          return UserAvatar(profile: profile, size: Spacings.l);
        },
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
      direction: ContextMenuDirection.right,
      controller: contextMenuController,
      menuItems: [
        ContextMenuItem(
          label: loc.chatList_newContact,
          leading: const AppIcon.user(size: 16),
          onPressed: () {
            _newContact(context);
          },
        ),
        ContextMenuItem(
          label: loc.chatList_newGroup,
          leading: const AppIcon.users(size: 16),
          onPressed: () {
            _newGroup(context);
          },
        ),
      ],
      // The plus button is different on mobile and desktop
      child: isSmallScreen(context)
          ? GlassCircleButton(
              icon: const AppIcon.plus(size: 20),
              onPressed: contextMenuController.show,
            )
          : GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: contextMenuController.show,
              child: const SizedBox(
                width: _kHeaderSlotSize,
                height: _kHeaderSlotSize,
                child: Center(child: AppIcon.plus(size: 20)),
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
