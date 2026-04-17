// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/user.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';

class MockInvitationCodesCubit extends MockCubit<InvitationCodesState>
    implements InvitationCodesCubit {}

void main() {
  group('InvitationCodesScreenTest', () {
    late MockInvitationCodesCubit cubit;

    setUp(() {
      cubit = MockInvitationCodesCubit();
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<AppLocaleCubit>(create: (_) => AppLocaleCubit()),
        BlocProvider<InvitationCodesCubit>.value(value: cubit),
      ],
      child: Builder(
        builder: (context) => MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const InvitationCodesView(),
        ),
      ),
    );

    final fixedDate = DateTime(2026, 1, 1);

    UiInvitationCode token(int id) =>
        UiInvitationCode.token(TokenId(id: id, createdAt: fixedDate));

    UiInvitationCode code(String codeStr, {bool copied = false}) =>
        UiInvitationCode.code(
          InvitationCode(code: codeStr, copied: copied, createdAt: fixedDate),
        );

    void setState(List<UiInvitationCode> codes) {
      when(() => cubit.state).thenReturn(InvitationCodesState(codes: codes));
    }

    testWidgets('empty', (tester) async {
      setState([]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_empty.png'),
      );
    });

    testWidgets('single token', (tester) async {
      setState([token(1)]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_single_token.png'),
      );
    });

    testWidgets('single code', (tester) async {
      setState([code('ABCD-EFGH-IJKL')]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_single_code.png'),
      );
    });

    testWidgets('single copied code', (tester) async {
      setState([code('ABCD-EFGH-IJKL', copied: true)]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_single_copied_code.png'),
      );
    });

    testWidgets('multiple tokens', (tester) async {
      setState([token(1), token(2), token(3)]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_multiple_tokens.png'),
      );
    });

    testWidgets('multiple codes', (tester) async {
      setState([
        code('ABCD-EFGH-IJKL'),
        code('MNOP-QRST-UVWX'),
        code('YZAB-CDEF-GHIJ', copied: true),
      ]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_multiple_codes.png'),
      );
    });

    testWidgets('mixed tokens and codes', (tester) async {
      setState([
        code('ABCD-EFGH-IJKL'),
        token(1),
        code('MNOP-QRST-UVWX', copied: true),
        token(2),
      ]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invite_codes_screen_mixed.png'),
      );
    });
  });
}
