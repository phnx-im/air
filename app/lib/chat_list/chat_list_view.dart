// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';

import 'chat_list_content.dart';
import 'chat_list_cubit.dart';
import 'chat_list_header.dart';

class ChatListContainer extends StatelessWidget {
  const ChatListContainer({required this.isStandalone, super.key});

  final bool isStandalone;

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) => ChatListCubit(userCubit: context.read<UserCubit>()),
      child: ChatListView(scaffold: isStandalone),
    );
  }
}

class ChatListView extends StatelessWidget {
  const ChatListView({
    super.key,
    this.scaffold = false,
    this.createChatDetailsCubit = ChatDetailsCubit.new,
  });

  final bool scaffold;
  final ChatDetailsCubitCreate createChatDetailsCubit;

  @override
  Widget build(BuildContext context) {
    final widget = Container(
      color: CustomColorScheme.of(context).backgroundBase.primary,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const ChatListHeader(),
          Expanded(
            child: ChatListContent(
              createChatDetailsCubit: createChatDetailsCubit,
            ),
          ),
        ],
      ),
    );
    return scaffold
        ? Scaffold(
            backgroundColor: CustomColorScheme.of(
              context,
            ).backgroundBase.primary,
            body: Stack(
              children: [
                SafeArea(bottom: false, child: widget),
                const SafeArea(child: SizedBox.expand()),
              ],
            ),
          )
        : widget;
  }
}
