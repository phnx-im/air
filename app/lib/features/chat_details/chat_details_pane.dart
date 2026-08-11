// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';

import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/group_details_view.dart';

/// The bottom level of the chat details: a contact's profile or a group's
/// details.
class ChatDetailsPane extends StatelessWidget {
  const ChatDetailsPane({super.key});

  @override
  Widget build(BuildContext context) {
    final chatType = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.chatType,
    );

    return switch (chatType) {
      UiChatType_Connection(field0: final profile) ||
      UiChatType_TargetedMessageConnection(
        field0: final profile,
      ) => _ContactPane(profile: profile),
      UiChatType_Group() => const _GroupPane(),
      UiChatType_HandleConnection() ||
      UiChatType_PendingConnection() ||
      null => const _UnknownChatPane(),
    };
  }
}

/// The profile of the person on the other end of a one-to-one chat.
class _ContactPane extends StatelessWidget {
  const _ContactPane({required this.profile});

  final UiUserProfile profile;

  @override
  Widget build(BuildContext context) {
    final chat = context.select((ChatDetailsCubit cubit) => cubit.state.chat);
    if (chat == null) {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);

    return ModalPane(
      title: loc.contactDetailsScreen_title,
      child: ContactDetailsView(
        profile: profile,
        relationship: ContactRelationship(
          contactChatId: chat.id,
          isBlocked: chat.status == const UiChatStatus.blocked(),
        ),
      ),
    );
  }
}

class _GroupPane extends StatelessWidget {
  const _GroupPane();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ModalPane(
      title: loc.groupDetails_title,
      child: const GroupDetailsView(),
    );
  }
}

/// Fallback for a connection that has not been accepted yet.
class _UnknownChatPane extends StatelessWidget {
  const _UnknownChatPane();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ModalPane(
      title: loc.contactDetailsScreen_title,
      child: Center(child: Text(loc.chatDetailsScreen_unknownChat)),
    );
  }
}
