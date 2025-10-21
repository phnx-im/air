// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/navigation/navigation.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/widgets/widgets.dart';

import 'connection_details.dart';
import 'chat_details_cubit.dart';
import 'group_details.dart';

/// Container for [ChatDetailsScreenView]
///
/// Wraps the screen with required providers.
class ChatDetailsScreen extends StatelessWidget {
  const ChatDetailsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final chatId = context.select(
      (NavigationCubit cubit) => cubit.state.chatId,
    );

    if (chatId == null) {
      return const SizedBox.shrink();
    }

    return BlocProvider(
      create: (context) {
        return ChatDetailsCubit(
          userCubit: context.read<UserCubit>(),
          chatId: chatId,
        );
      },
      child: const ChatDetailsScreenView(),
    );
  }
}

/// Screen that shows details of a chat
class ChatDetailsScreenView extends StatelessWidget {
  const ChatDetailsScreenView({super.key});

  @override
  Widget build(BuildContext context) {
    final chatExists = context.select(
      (NavigationCubit cubit) => switch (cubit.state) {
        NavigationState_Intro() => false,
        NavigationState_Home(:final home) => home.chatId != null,
      },
    );
    if (!chatExists) {
      return const SizedBox.shrink();
    }

    final chatType = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.chatType,
    );

    final loc = AppLocalizations.of(context);

    return Scaffold(
      appBar: AppBar(
        elevation: 0,
        scrolledUnderElevation: 0,
        leading: const AppBarBackButton(),
        title: Text(loc.chatDetailsScreen_title),
      ),
      body: SafeArea(
        child: switch (chatType) {
          UiChatType_HandleConnection() ||
          UiChatType_Connection() => const ConnectionDetails(),
          UiChatType_Group() => const GroupDetails(),
          null => Center(child: Text(loc.chatDetailsScreen_unknownChat)),
        },
      ),
    );
  }
}
