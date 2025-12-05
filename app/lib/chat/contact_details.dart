// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/widgets/remove_member_button.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';

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
}
