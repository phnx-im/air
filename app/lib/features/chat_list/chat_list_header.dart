// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/features/chat_list/add_contact_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/list_header/list_header.dart';
import 'package:air/ds/patterns/list_header/list_header_tokens.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/share/share_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class ChatListHeader extends StatelessWidget {
  const ChatListHeader({super.key, this.scrollOffset});

  /// The list's scroll offset, which reveals the title pill.
  final ValueNotifier<double>? scrollOffset;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final tokens = ListHeaderTokens.current;
    final sharePending = context.select(
      (AndroidShareCubit cubit) => cubit.state != null,
    );
    return ListHeader(
      tokens: tokens,
      title: sharePending ? loc.shareBanner_chooseChat : loc.homeTab_chats,
      scrollOffset: scrollOffset,
      leading: sharePending ? null : _ComposeButton(tokens: tokens),
    );
  }
}

class _ComposeButton extends StatelessWidget {
  const _ComposeButton({required this.tokens});

  final ListHeaderTokens tokens;

  @override
  Widget build(BuildContext context) {
    return ListHeaderAction(tokens: tokens, onAction: _openMenu);
  }

  /// [buttonContext] is the button's own, so the menu hangs off the button
  /// rather than off the header that lays it out.
  void _openMenu(BuildContext buttonContext) {
    final render = buttonContext.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;
    final loc = AppLocalizations.of(buttonContext);

    unawaited(
      showOverlayMenu(
        context: buttonContext,
        anchor: render.localToGlobal(Offset.zero) & render.size,
        items: [
          MenuItem(
            label: loc.chatList_newContact,
            leading: const AppIcon.user(size: 16),
            onPressed: () => _newContact(buttonContext),
          ),
          MenuItem(
            label: loc.chatList_newGroup,
            leading: const AppIcon.users(size: 16),
            onPressed: () => _newGroup(buttonContext),
          ),
        ],
      ),
    );
  }

  void _newContact(BuildContext context) {
    showDialog(
      context: context,
      builder: (BuildContext context) => const AddContactDialog(),
    );
  }

  void _newGroup(BuildContext context) {
    context.read<NavigationCubit>().openCreateGroup();
  }
}
