// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat_details/safety_code_screen.dart';
import 'package:air/features/chat_details/mute_button.dart';
import 'package:air/features/chat_details/remove_member_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
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
    final loc = AppLocalizations.of(context);
    return Align(
      alignment: Alignment.topCenter,
      child: Column(
        children: [
          const SizedBox(height: S.s12),

          UserAvatar(profile: profile, size: 192),

          const SizedBox(height: S.s16),

          Text(
            profile.displayName,
            style: typeScale.header.xl.style(weight: Weight.emphasized),
          ),

          const SizedBox(height: S.s16),

          OutlinedButton(
            onPressed: () => _handleChat(context),
            style: const ButtonStyle(
              visualDensity: VisualDensity.compact,
              minimumSize: WidgetStatePropertyAll(Size(82, 32)),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const AppIcon.messageCircle(size: 16),
                const SizedBox(width: S.s8),
                Text(
                  loc.contactDetailsScreen_chat,
                  style: typeScale.body.regular.style(),
                ),
              ],
            ),
          ),

          const SizedBox(height: S.s16),

          OutlinedButton(
            onPressed: () => _handleViewSafetyNumber(context, profile.userId),
            style: const ButtonStyle(
              visualDensity: VisualDensity.compact,
              minimumSize: WidgetStatePropertyAll(Size(82, 32)),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const AppIcon.shield(size: 16),
                const SizedBox(width: S.s8),
                Text(
                  loc.contactDetailsScreen_viewSafetyCode,
                  style: typeScale.body.regular.style(),
                ),
              ],
            ),
          ),

          if (relationship is ContactRelationship) ...[
            const SizedBox(height: S.s16),
            const MuteButton(),
          ],

          const Spacer(),

          ReportSpamButton(userId: profile.userId),

          if (relationship case ContactRelationship())
            if (relationship case ContactRelationship(:final isBlocked)) ...[
              const SizedBox(height: S.s16),
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
            const SizedBox(height: S.s16),
            DeleteContactButton(
              chatId: contactChatId,
              displayName: profile.displayName,
            ),
          ],

          if (relationship case MemberRelationship(
            :final groupChatId,
            :final canKick,
          ) when canKick) ...[
            const SizedBox(height: S.s16),
            RemoveMemberButton(
              chatId: groupChatId,
              memberId: profile.userId,
              displayName: profile.displayName,
              enabled: true,
              onRemoved: () {
                if (Navigator.of(context).canPop()) {
                  Navigator.of(context).pop();
                }
              },
            ),
          ],
        ],
      ),
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

  void _handleViewSafetyNumber(BuildContext context, UiUserId userId) async {
    Navigator.of(
      context,
    ).push(MaterialPageRoute(builder: (_) => SafetyCodeScreen(userId: userId)));
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

    final colors = SemanticColors.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
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
            style: typeScale.body.regular.style(color: colors.text.secondary),
          ),

          const SizedBox(height: S.s24),

          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  child: Text(
                    loc.addContactDialog_cancel,
                    style: typeScale.body.regular.style(),
                  ),
                ),
              ),

              const SizedBox(width: S.s12),

              Expanded(
                child: AppDialogProgressButton(
                  onPressed: (inProgress) =>
                      _handleSendChatRequest(context, inProgress),
                  progressColor: colors.function.neutral.toggleWhite,
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accentBrand.primary,
                    ),
                    overlayColor: WidgetStatePropertyAll(
                      colors.accentBrand.primary,
                    ),
                    foregroundColor: WidgetStatePropertyAll(
                      colors.function.neutral.toggleWhite,
                    ),
                  ),
                  child: Text(
                    loc.addContactDialog_confirm,
                    style: typeScale.body.regular.style(),
                  ),
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
    ValueNotifier<bool> inProgress,
  ) async {
    inProgress.value = true;

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
      inProgress.value = false;
    }
  }
}
