// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat_details/delete_contact_button.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('DeleteContactButton', () {
    late MockUserCubit userCubit;
    late MockNavigationCubit navigationCubit;
    final chatId = 1.chatId();

    setUpAll(() => registerFallbackValue(0.chatId()));

    setUp(() {
      userCubit = MockUserCubit();
      navigationCubit = MockNavigationCubit();
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
      ],
      child: MaterialApp(
        theme: testLightTheme,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(
          body: DeleteContactButton(chatId: chatId, displayName: 'Alice'),
        ),
      ),
    );

    Future<void> confirmDeletion(WidgetTester tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.tap(find.text('Delete'));
      await tester.pumpAndSettle();
      // The dialog's confirm button sits above the button that opened it.
      await tester.tap(find.text('Delete').last);
      await tester.pumpAndSettle();
    }

    testWidgets('deletes the contact and closes its chat once confirmed', (
      tester,
    ) async {
      when(() => userCubit.deleteChat(any())).thenAnswer((_) async {});

      await confirmDeletion(tester);

      verify(() => userCubit.deleteChat(chatId)).called(1);
      verify(() => navigationCubit.closeChat()).called(1);
    });

    testWidgets('keeps the chat open when the deletion fails', (tester) async {
      when(() => userCubit.deleteChat(any()))
          .thenAnswer((_) async => throw Exception('offline'));

      await confirmDeletion(tester);

      verify(() => userCubit.deleteChat(chatId)).called(1);
      verifyNever(() => navigationCubit.closeChat());
    });
  });
}
