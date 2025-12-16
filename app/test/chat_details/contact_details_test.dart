// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/contact_details.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';

import '../chat_list/chat_list_content_test.dart';

final chat = chats[0];

void main() {
  group('ContactDetails', () {
    Widget buildSubject(Relationship relationship) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: lightTheme,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: AppScaffold(
            child: ContactDetailsView(
              profile: userProfiles[1],
              relationship: relationship,
            ),
          ),
        );
      },
    );

    testWidgets('renders correctly (contact)', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_contact.png'),
      );
    });

    testWidgets('renders correctly (contact, blocked)', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: true),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_contact_blocked.png'),
      );
    });

    testWidgets('renders correctly (member)', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          MemberRelationship(
            groupChatId: chat.id,
            groupTitle: 'Group',
            canKick: true,
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_member.png'),
      );
    });

    testWidgets('renders correctly (member, no kick)', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          MemberRelationship(
            groupChatId: chat.id,
            groupTitle: 'Group',
            canKick: false,
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_member_no_kick.png'),
      );
    });
  });
}
