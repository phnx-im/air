// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card_tokens.dart';
import 'package:air/features/chat/contact_request_actions.dart';
import 'package:air/features/message_list/contact_request_dialog.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';

/// Who a sent contact request went to.
sealed class SentContactRequestRecipient {
  const SentContactRequestRecipient();

  /// Sent to a username.
  const factory SentContactRequestRecipient.username(UiUsername username) =
      _UsernameRecipient;

  /// Sent to a member of a group chat, whose profile the two already share.
  /// [groupChatTitle] is null when the group chat is not on this device.
  const factory SentContactRequestRecipient.user(
    UiUserId userId, {
    String? groupChatTitle,
  }) = _UserRecipient;
}

class _UsernameRecipient extends SentContactRequestRecipient {
  const _UsernameRecipient(this.username);

  final UiUsername username;
}

class _UserRecipient extends SentContactRequestRecipient {
  const _UserRecipient(this.userId, {this.groupChatTitle});

  final UiUserId userId;
  final String? groupChatTitle;
}

/// The card of a contact request the user sent and the recipient has not
/// answered yet. The only action is to take the request back.
class SentContactRequestCard extends StatelessWidget {
  const SentContactRequestCard({
    super.key,
    required this.chatId,
    required this.recipient,
  });

  final ChatId chatId;
  final SentContactRequestRecipient recipient;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final actions = ContactRequestRetraction(
      label: loc.sentContactRequest_retract,
      onRetract: () => retractContactRequest(context, chatId),
    );

    return switch (recipient) {
      _UsernameRecipient(:final username) => ContactRequestCard(
        tokens: ContactRequestCardTokens.current,
        title: loc.sentContactRequest_title,
        subtitle: loc.sentContactRequest_toUsername(username.plaintext),
        body: loc.sentContactRequest_body,
        displayName: username.plaintext,
        actions: actions,
      ),
      _UserRecipient(:final userId, :final groupChatTitle) => _UserRequestCard(
        userId: userId,
        groupChatTitle: groupChatTitle,
        actions: actions,
      ),
    };
  }
}

class _UserRequestCard extends StatelessWidget {
  const _UserRequestCard({
    required this.userId,
    required this.groupChatTitle,
    required this.actions,
  });

  final UiUserId userId;
  final String? groupChatTitle;
  final ContactRequestCardActions actions;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final profile = context.select(
      (UsersCubit c) => c.state.profile(userId: userId),
    );
    final groupChatTitle = this.groupChatTitle;

    return ContactRequestCard(
      tokens: ContactRequestCardTokens.current,
      title: loc.sentContactRequest_titleWithName(profile.displayName),
      subtitle: groupChatTitle == null
          ? null
          : loc.sentContactRequest_viaGroup(groupChatTitle),
      body: loc.sentContactRequest_body,
      displayName: profile.displayName,
      gradientSeed: profile.userId.uuid.uuid,
      image: contactRequestPicture(context, profile.profilePicture),
      actions: actions,
    );
  }
}
