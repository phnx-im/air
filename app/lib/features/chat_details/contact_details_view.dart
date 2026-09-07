// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat_details/mute_button.dart';
import 'package:air/features/chat_details/remove_member_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/button_cta/button_cta.dart';
import 'package:air/ds/components/button_cta/button_cta_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/features/user/avatar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

import 'package:air/features/chat_details/block_contact_button.dart';
import 'package:air/features/chat_details/delete_contact_button.dart';
import 'package:air/features/chat_details/report_spam_button.dart';
import 'package:air/features/chat_details/unblock_contact_button.dart';
import 'package:air/features/developer/chat_debug_info_view.dart'
    show ChatDebugInfoRow;

final _log = Logger("ContactDetails");

/// Either a direct contact or a member of a group
sealed class Relationship {
  const Relationship();
}

class ContactRelationship extends Relationship {
  const ContactRelationship({
    required this.contactChatId,
    required this.isBlocked,
  });

  final ChatId contactChatId;
  final bool isBlocked;

  @override
  String toString() =>
      'ContactRelationship(contactChatId: $contactChatId, isBlocked: $isBlocked)';
}

class MemberRelationship extends Relationship {
  const MemberRelationship({
    required this.groupChatId,
    required this.groupTitle,
    required this.canKick,
  });

  final ChatId groupChatId;
  final String groupTitle;
  final bool canKick;

  @override
  String toString() =>
      'MemberRelationship(groupChatId: $groupChatId, groupTitle: $groupTitle, canKick: $canKick)';
}

/// Body of the profile modal page, shared by a one-to-one chat and a group
/// member. Content only: surface, header, and scrolling are the modal's.
class ContactDetailsView extends StatelessWidget {
  const ContactDetailsView({
    super.key,
    required this.profile,
    required this.relationship,
  });

  final UiUserProfile profile;
  final Relationship relationship;

  @override
  Widget build(BuildContext context) {
    return ModalBody(
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          Center(child: UserAvatar(profile: profile, size: 192)),

          const SizedBox(height: S.s16),

          Text(
            profile.displayName,
            textAlign: .center,
            style: typeScale.header.xl.style(weight: Weight.emphasized),
          ),

          const SizedBox(height: S.s24),

          _CallToActions(profile: profile, relationship: relationship),

          const SizedBox(height: S.s24),

          _Actions(profile: profile, relationship: relationship),

          // Only for a contact: a member's pane is scoped to the group, so the
          // chat behind it is not this profile's.
          if (relationship is ContactRelationship) const ChatDebugInfoRow(),
        ],
      ),
    );
  }
}

/// The round actions the profile leads with.
class _CallToActions extends StatelessWidget {
  const _CallToActions({required this.profile, required this.relationship});

  final UiUserProfile profile;
  final Relationship relationship;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final tokens = ButtonCTATokens.current;

    return Row(
      mainAxisAlignment: .center,
      children: [
        // Flexible so a long label wraps under its circle rather than pushing
        // the row past the card's width.
        Flexible(
          child: ButtonCTA(
            tokens: tokens,
            label: loc.contactDetailsScreen_chat,
            icon: AppIconType.messageCircle,
            type: ButtonCTAType.secondary,
            onPressed: () => _handleChat(context),
          ),
        ),

        const SizedBox(width: S.s32),

        Flexible(
          child: ButtonCTA(
            tokens: tokens,
            label: loc.contactDetailsScreen_viewSafetyCode,
            icon: AppIconType.shield,
            type: ButtonCTAType.secondary,
            onPressed: () =>
                context.read<NavigationCubit>().openSafetyCode(profile.userId),
          ),
        ),

        if (relationship is ContactRelationship) ...[
          const SizedBox(width: S.s32),
          const Flexible(child: MuteButton()),
        ],
      ],
    );
  }

  void _handleChat(BuildContext context) async {
    switch (relationship) {
      case ContactRelationship(:final contactChatId):
        final navigationCubit = context.read<NavigationCubit>();
        navigationCubit.openChat(contactChatId);
        return;

      case MemberRelationship(:final groupChatId, :final groupTitle):
        final userCubit = context.read<UserCubit>();
        final contact = await userCubit.contact(userId: profile.userId);

        if (!context.mounted) return;

        if (contact != null) {
          final navigationCubit = context.read<NavigationCubit>();
          navigationCubit.openChat(contact.chatId);
          return;
        }

        // No contact found means we can establish a new connection
        showDialog(
          context: context,
          builder: (context) => _AddContactDialog(
            userId: profile.userId,
            displayName: profile.displayName,
            groupChatId: groupChatId,
            groupTitle: groupTitle,
          ),
        );
    }
  }
}

/// The stacked actions the profile closes with, each one full width.
class _Actions extends StatelessWidget {
  const _Actions({required this.profile, required this.relationship});

  final UiUserProfile profile;
  final Relationship relationship;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        ReportSpamButton(userId: profile.userId),

        if (relationship case ContactRelationship(:final isBlocked)) ...[
          const SizedBox(height: S.s12),
          isBlocked
              ? UnblockContactButton(
                  userId: profile.userId,
                  displayName: profile.displayName,
                )
              : BlockContactButton(
                  userId: profile.userId,
                  displayName: profile.displayName,
                ),
        ],

        if (relationship case ContactRelationship(:final contactChatId)) ...[
          const SizedBox(height: S.s12),
          DeleteContactButton(
            chatId: contactChatId,
            displayName: profile.displayName,
          ),
        ],

        if (relationship case MemberRelationship(
          :final groupChatId,
          :final canKick,
        ) when canKick) ...[
          const SizedBox(height: S.s12),
          RemoveMemberButton(
            chatId: groupChatId,
            memberId: profile.userId,
            displayName: profile.displayName,
            enabled: true,
            onRemoved: () => context.read<NavigationCubit>().pop(),
          ),
        ],
      ],
    );
  }
}

class _AddContactDialog extends HookWidget {
  const _AddContactDialog({
    required this.userId,
    required this.displayName,
    required this.groupChatId,
    required this.groupTitle,
  });

  final UiUserId userId;
  final String displayName;
  final ChatId groupChatId;
  final String groupTitle;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final palette = SemanticPalette.of(context);
    final inProgress = useState(false);

    return AppDialog(
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .start,
        children: [
          Center(
            child: Text(
              loc.addContactDialog_title,
              style: typeScale.header.regular.style(weight: Weight.emphasized),
            ),
          ),

          const SizedBox(height: S.s8),

          Text(
            loc.addContactDialog_content(displayName, groupTitle),
            style: typeScale.body.regular.style(color: palette.text.secondary),
          ),

          const SizedBox(height: S.s24),

          Row(
            children: [
              Expanded(
                child: Button(
                  onPressed: () => Navigator.of(context).pop(false),
                  label: loc.addContactDialog_cancel,
                  type: ButtonType.secondary,
                ),
              ),

              const SizedBox(width: S.s12),

              Expanded(
                child: Button(
                  onPressed: () => _handleSendChatRequest(
                    context,
                    (value) => inProgress.value = value,
                  ),
                  label: loc.addContactDialog_confirm,
                  state: inProgress.value
                      ? ButtonState.pending
                      : ButtonState.active,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  void _handleSendChatRequest(
    BuildContext context,
    void Function(bool) setInProgress,
  ) async {
    setInProgress(true);

    final userCubit = context.read<UserCubit>();
    final navigationCubit = context.read<NavigationCubit>();

    try {
      final chatId = await userCubit.addContactFromGroup(
        userId: userId,
        chatId: groupChatId,
      );
      navigationCubit.openChat(chatId);
    } catch (error) {
      _log.severe("Failed to send contact request: ${error.toString()}");

      if (context.mounted) {
        Navigator.of(context).pop();
      }

      showErrorBannerStandalone(
        (loc) => loc.newConnectionDialog_error(displayName),
      );
    } finally {
      setInProgress(false);
    }
  }
}
