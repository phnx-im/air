// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/chat/contact_details.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/app_scaffold.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../helpers.dart';
import '../mocks.dart';

final chat = chats[0];

void main() {
  group('ContactDetails', () {
    late MockChatDetailsCubit chatDetailsCubit;

    setUp(() {
      chatDetailsCubit = MockChatDetailsCubit();
    });

    Widget buildSubject(
      Relationship relationship, {
      UiChatMuted? mutedUntil,
    }) {
      when(() => chatDetailsCubit.state).thenReturn(
        ChatDetailsState(
          chat: UiChatDetails(
            id: chat.id,
            status: chat.status,
            chatType: chat.chatType,
            lastUsed: chat.lastUsed,
            messagesCount: chat.messagesCount,
            unreadMessages: chat.unreadMessages,
            lastMessage: chat.lastMessage,
            draft: chat.draft,
            isApq: chat.isApq,
            mutedUntil: mutedUntil,
          ),
          members: const [],
        ),
      );
      return BlocProvider<ChatDetailsCubit>.value(
        value: chatDetailsCubit,
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: AppScaffold(
            child: ContactDetailsView(
              profile: userProfiles[1],
              relationship: relationship,
            ),
          ),
        ),
      );
    }

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

    testWidgets('renders correctly (contact, muted until)', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
          mutedUntil: UiChatMuted.until(DateTime(9999)),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_contact_muted_until.png'),
      );
    });

    testWidgets('renders correctly (contact, muted forever)', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
          mutedUntil: const UiChatMuted.forever(),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_contact_muted_forever.png'),
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
