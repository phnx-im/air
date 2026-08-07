// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat_details/add_members_cubit.dart';
import 'package:air/features/chat_details/add_members_screen.dart';
import 'package:air/features/chat_details/member_selection_list.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';

import '../chat_list/chat_list_content_test.dart';
import '../../helpers.dart';
import '../../mocks.dart';

final _chat = chats[2];

const _features = AirFeatures(
  encryptedGroupProfiles: true,
  emptyConnectionGroupAttributes: true,
  pqGroups: true,
);

final _contacts = [
  UiContact(
    userId: 2.userId(),
    chatId: 2.chatId(),
    supportedFeatures: _features,
  ),
  UiContact(
    userId: 3.userId(),
    chatId: 3.chatId(),
    supportedFeatures: _features,
  ),
  UiContact(
    userId: 4.userId(),
    chatId: 4.chatId(),
    supportedFeatures: _features,
  ),
];

void main() {
  setUpAll(() {
    registerFallbackValue(0.userId());
  });

  group('AddMembersScreenView', () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockUsersCubit usersCubit;
    late AddMembersCubit addMembersCubit;

    setUp(() {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      usersCubit = MockUsersCubit();
      addMembersCubit = AddMembersCubit()
        ..loadContacts(Future.value(_contacts));
      addTearDown(addMembersCubit.close);

      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: _chat, members: const []));
      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: _chat.id)),
      );
      when(() => userCubit.state).thenReturn(MockUiUser(id: 1));
      when(
        () => usersCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
        BlocProvider<AddMembersCubit>.value(value: addMembersCubit),
      ],
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testLightTheme,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: const AddMembersScreenView(),
      ),
    );

    // The selection list only scrolls where the modal hands it a bounded
    // height. Neither breakpoint may leave it unbounded (an overflow) nor
    // collapse the card to its minimum (a list of a few pixels). The header
    // carries a label-bearing action, so it is also where a slot too narrow
    // for its action would show up.
    testWidgets('fills the full-screen modal', (tester) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expectFillsModal(tester, find.byType(MemberSelectionList), phoneViewSize);
      expect(
        tester.getCenter(find.text('Add members')).dx,
        moreOrLessEquals(phoneViewSize.width / 2),
      );
    });

    testWidgets('fills the card modal', (tester) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expectFillsModal(
        tester,
        find.byType(MemberSelectionList),
        desktopViewSize,
      );
      expect(
        tester.getCenter(find.text('Add members')).dx,
        moreOrLessEquals(desktopViewSize.width / 2),
      );
    });

    testWidgets('enables the done action once a contact is selected', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      Button done() => tester.widget<Button>(find.byType(Button));
      expect(done().state, ButtonState.disabled);

      await tester.tap(find.text('Bob'));
      await tester.pumpAndSettle();

      expect(done().state, ButtonState.active);
    });
  });
}
