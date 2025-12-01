// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/widgets/remove_member_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:provider/provider.dart';

import 'block_contact_button.dart';
import 'delete_contact_button.dart';
import 'report_spam_button.dart';
import 'unblock_contact_button.dart';

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
    return Align(
      alignment: Alignment.topCenter,
      child: Column(
        children: [
          const SizedBox(height: Spacings.s),

          UserAvatar(size: 192, userId: profile.userId, profile: profile),

          const SizedBox(height: Spacings.s),

          Text(
            profile.displayName,
            style: TextStyle(
              fontSize: HeaderFontSize.h1.size,
              fontWeight: FontWeight.bold,
            ),
          ),

          const SizedBox(height: Spacings.s),

          OutlinedButton(
            onPressed: () => _handleChat(context),
            style: const ButtonStyle(
              visualDensity: VisualDensity.compact,
              minimumSize: WidgetStatePropertyAll(Size(82, 32)),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                iconoir.ChatBubbleEmpty(
                  color: CustomColorScheme.of(context).text.primary,
                  width: 16,
                ),
                const SizedBox(width: Spacings.xxs),
                Text(
                  "Chat",
                  style: TextStyle(fontSize: LabelFontSize.base.size),
                ),
              ],
            ),
          ),

          const Spacer(),

          ReportSpamButton(userId: profile.userId),

          if (relationship case ContactRelationship())
            if (relationship case ContactRelationship(:final isBlocked)) ...[
              const SizedBox(height: Spacings.s),
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
            const SizedBox(height: Spacings.s),
            DeleteContactButton(
              chatId: contactChatId,
              displayName: profile.displayName,
            ),
          ],

          if (relationship case MemberRelationship(
            :final groupChatId,
            :final canKick,
          ) when canKick) ...[
            const SizedBox(height: Spacings.s),
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

      case MemberRelationship(:final groupTitle):
        final contact = await context.read<UserCubit>().contact(
          userId: profile.userId,
        );
        debugPrint("Contact: $contact");
        if (contact != null && context.mounted) {
          final navigationCubit = context.read<NavigationCubit>();
          navigationCubit.openChat(contact.chatId);
          return;
        }

        // No contact found means we can establish a new connection
        if (!context.mounted) return;
        showDialog(
          context: context,
          builder: (context) => _AddContactDialog(
            displayName: profile.displayName,
            groupTitle: groupTitle,
          ),
        );
    }
  }
}

class _AddContactDialog extends HookWidget {
  const _AddContactDialog({
    required this.displayName,
    required this.groupTitle,
  });

  final String displayName;
  final String groupTitle;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final colors = CustomColorScheme.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              loc.addContactDialog_title,
              style: TextStyle(
                fontSize: HeaderFontSize.h4.size,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),

          const SizedBox(height: Spacings.xxs),

          Text(
            loc.addContactDialog_content(displayName, groupTitle),
            style: TextStyle(
              color: colors.text.secondary,
              fontSize: BodyFontSize.base.size,
            ),
          ),

          const SizedBox(height: Spacings.m),

          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  child: Text(
                    loc.addContactDialog_cancel,
                    style: TextStyle(fontSize: LabelFontSize.base.size),
                  ),
                ),
              ),

              const SizedBox(width: Spacings.xs),

              Expanded(
                child: OutlinedButton(
                  onPressed: () => _handleSendChatRequest(context),
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.primary,
                    ),
                    overlayColor: WidgetStatePropertyAll(colors.accent.primary),
                    foregroundColor: WidgetStatePropertyAll(
                      colors.function.toggleWhite,
                    ),
                  ),
                  child: Text(
                    loc.addContactDialog_confirm,
                    style: TextStyle(fontSize: LabelFontSize.base.size),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  void _handleSendChatRequest(BuildContext context) async {
    // TODO: Implement
  }
}
