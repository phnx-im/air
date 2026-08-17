// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/member_details_cubit.dart';

/// The profile of a group member.
class MemberDetailsPane extends StatelessWidget {
  const MemberDetailsPane({
    super.key,
    required this.chatId,
    required this.memberId,
  });

  final ChatId chatId;
  final UiUserId memberId;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: memberId),
    );

    final groupTitle = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.title,
    );

    final canKick = context.select(
      (MemberDetailsCubit cubit) =>
          cubit.state.roomState?.canKick(target: memberId) ?? false,
    );

    final loc = AppLocalizations.of(context);

    return ModalPane(
      title: loc.contactDetailsScreen_title,
      child: ContactDetailsView(
        profile: profile,
        relationship: MemberRelationship(
          groupChatId: chatId,
          groupTitle: groupTitle ?? "",
          canKick: canKick,
        ),
      ),
    );
  }
}
