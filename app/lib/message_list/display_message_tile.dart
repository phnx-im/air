// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/users_cubit.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import 'timestamp.dart';

class DisplayMessageTile extends StatefulWidget {
  final UiEventMessage eventMessage;
  final DateTime timestamp;
  const DisplayMessageTile(this.eventMessage, this.timestamp, {super.key});

  @override
  State<DisplayMessageTile> createState() => _DisplayMessageTileState();
}

class _DisplayMessageTileState extends State<DisplayMessageTile> {
  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(vertical: Spacings.m),
      child: Column(
        spacing: Spacings.xxxs,
        children: [
          Container(
            child: switch (widget.eventMessage) {
              UiEventMessage_System(field0: final message) =>
                SystemMessageContent(message: message),
              UiEventMessage_Error(field0: final message) =>
                ErrorMessageContent(message: message),
            },
          ),
          Timestamp(widget.timestamp),
        ],
      ),
    );
  }
}

class SystemMessageContent extends StatelessWidget {
  const SystemMessageContent({super.key, required this.message});

  final UiSystemMessage message;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final textStyle = TextStyle(
      color: CustomColorScheme.of(context).text.tertiary,
      fontSize: LabelFontSize.small1.size,
    );

    final profileNameStyle = textStyle.copyWith(fontWeight: FontWeight.bold);

    final messageText = switch (message) {
      UiSystemMessage_Add(:final field0, :final field1) => () {
        final user1Name = context.select(
          (UsersCubit c) => c.state.profile(userId: field0).displayName,
        );
        final user2Name = context.select(
          (UsersCubit c) => c.state.profile(userId: field1).displayName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userAddedUser_prefix(user1Name),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userAddedUser_infix),
              TextSpan(
                text: loc.systemMessage_userAddedUser_suffix(user2Name),
                style: profileNameStyle,
              ),
            ],
          ),
        );
      }(),
      UiSystemMessage_Remove(:final field0, :final field1) => () {
        final user1Name = context.select(
          (UsersCubit c) => c.state.profile(userId: field0).displayName,
        );
        final user2Name = context.select(
          (UsersCubit c) => c.state.profile(userId: field1).displayName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userRemovedUser_prefix(user1Name),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userRemovedUser_infix),
              TextSpan(
                text: loc.systemMessage_userRemovedUser_suffix(user2Name),
                style: profileNameStyle,
              ),
            ],
          ),
        );
      }(),
      UiSystemMessage_ChangeTitle(
        :final field0,
        :final field1,
        :final field2,
      ) =>
        () {
          final userName = context.select(
            (UsersCubit c) => c.state.profile(userId: field0).displayName,
          );
          return RichText(
            text: TextSpan(
              style: textStyle,
              children: [
                TextSpan(
                  text: loc.systemMessage_userChangedTitle_prefix(userName),
                  style: profileNameStyle,
                ),
                TextSpan(text: loc.systemMessage_userChangedTitle_infix_1),
                TextSpan(
                  text: loc.systemMessage_userChangedTitle_infix_2(field1),
                  style: profileNameStyle,
                ),
                TextSpan(text: loc.systemMessage_userChangedTitle_infix_3),
                TextSpan(
                  text: loc.systemMessage_userChangedTitle_suffix(field2),
                  style: profileNameStyle,
                ),
              ],
            ),
          );
        }(),
      UiSystemMessage_ChangePicture(:final field0) => () {
        final userName = context.select(
          (UsersCubit c) => c.state.profile(userId: field0).displayName,
        );
        return RichText(
          text: TextSpan(
            style: textStyle,
            children: [
              TextSpan(
                text: loc.systemMessage_userChangedPicture_prefix(userName),
                style: profileNameStyle,
              ),
              TextSpan(text: loc.systemMessage_userChangedPicture_infix),
            ],
          ),
        );
      }(),
    };

    return Center(
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(Spacings.s),
          border: Border.all(
            color: CustomColorScheme.of(context).separator.secondary,
            width: 2,
          ),
        ),
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.xs,
        ),
        child: messageText,
      ),
    );
  }
}

class ErrorMessageContent extends StatelessWidget {
  const ErrorMessageContent({super.key, required this.message});

  final UiErrorMessage message;

  @override
  Widget build(BuildContext context) {
    return Container(
      alignment: AlignmentDirectional.topStart,
      child: Text(
        message.message,
        style: TextStyle(
          color: AppColors.red,
          fontSize: LabelFontSize.small2.size,
          height: 1.0,
        ),
      ),
    );
  }
}
