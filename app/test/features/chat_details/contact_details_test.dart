// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/contact_details_view.dart';
import 'package:air/features/chat_details/mute_button.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

final chat = chats[0];

const desktopPhysicalSize = Size(1400, 1000);

void main() {
  group('ContactDetails', () {
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() {
      chatDetailsCubit = MockChatDetailsCubit();
      userSettingsCubit = MockUserSettingsCubit();
    });

    Widget buildSubject(
      Relationship relationship, {
      UiChatMuted? mutedUntil,
      bool developerMode = false,
    }) {
      when(
        () => userSettingsCubit.state,
      ).thenReturn(UserSettings(developerMode: developerMode));
      when(() => chatDetailsCubit.state).thenReturn(
        ChatDetailsState(
          chat: UiChatDetails(
            id: chat.id,
            status: chat.status,
            chatType: chat.chatType,
            lastUsed: chat.lastUsed,
            unreadMessages: chat.unreadMessages,
            lastMessage: chat.lastMessage,
            draft: chat.draft,
            isApq: chat.isApq,
            mutedUntil: mutedUntil,
            pendingCommitFailed: false,
          ),
          members: const [],
        ),
      );
      return MultiBlocProvider(
        providers: [
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: ModalScaffold(
            title: 'Profile',
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

    testWidgets('renders correctly with mute menu open (mobile)', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
        ),
      );

      await tester.tap(find.byType(MuteButton));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_mute_menu_mobile.png'),
      );
    });

    testWidgets('renders correctly with mute menu open (desktop)', (
      tester,
    ) async {
      tester.view.physicalSize = desktopPhysicalSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
        ),
      );

      await tester.tap(find.byType(MuteButton));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_details_mute_menu_desktop.png'),
      );
    }, variant: desktopPlatform);

    testWidgets('debug info is absent outside developer mode', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
        ),
      );

      expect(find.text('Debug info'), findsNothing);
    });

    testWidgets('debug info renders in developer mode', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          ContactRelationship(contactChatId: chat.id, isBlocked: false),
          developerMode: true,
        ),
      );

      expect(find.text('Debug info'), findsOneWidget);
    });

    testWidgets('a member carries no debug info', (tester) async {
      // The member pane is scoped to the group, so the chat behind it is not
      // this profile's.
      await tester.pumpWidget(
        buildSubject(
          MemberRelationship(
            groupChatId: chat.id,
            groupTitle: 'Group',
            canKick: true,
          ),
          developerMode: true,
        ),
      );

      expect(find.text('Debug info'), findsNothing);
    });
  });
}
