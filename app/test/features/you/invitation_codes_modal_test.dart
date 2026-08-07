// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/you/invitation_codes_cubit.dart';
import 'package:air/features/you/invitation_codes_modal.dart';
import 'package:bloc_test/bloc_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';

class MockInvitationCodesCubit extends MockCubit<InvitationCodesState>
    implements InvitationCodesCubit {}

final fixedDate = DateTime(2026, 1, 1);

UiInvitationCode token(int id) =>
    UiInvitationCode.token(TokenId(id: id, createdAt: fixedDate));

UiInvitationCode code(String codeStr, {bool copied = false}) =>
    UiInvitationCode.code(
      InvitationCode(code: codeStr, copied: copied, createdAt: fixedDate),
    );

void main() {
  group('InvitationCodesModalTest', () {
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
          home: const InvitationCodesModal(),
        ),
      ),
    );

    void setState(List<UiInvitationCode> codes) {
      when(() => cubit.state).thenReturn(InvitationCodesState(codes: codes));
    }

    testWidgets('empty', (tester) async {
      setState([]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invitation_codes_modal_empty.png'),
      );
    });

    testWidgets('single token', (tester) async {
      setState([token(1)]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invitation_codes_modal_single_token.png'),
      );
    });

    testWidgets('single code', (tester) async {
      setState([code('ABCD-EFGH-IJKL')]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invitation_codes_modal_single_code.png'),
      );
    });

    testWidgets('single copied code', (tester) async {
      setState([code('ABCD-EFGH-IJKL', copied: true)]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invitation_codes_modal_single_copied_code.png'),
      );
    });

    testWidgets('multiple tokens', (tester) async {
      setState([token(1), token(2), token(3)]);
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/invitation_codes_modal_multiple_tokens.png'),
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
        matchesGoldenFile('goldens/invitation_codes_modal_multiple_codes.png'),
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
        matchesGoldenFile('goldens/invitation_codes_modal_mixed.png'),
      );
    });
  });

  group('showInvitationCodes', () {
    late MockInvitationCodesCubit cubit;

    setUp(() {
      cubit = MockInvitationCodesCubit();
      when(
        () => cubit.state,
      ).thenReturn(InvitationCodesState(codes: [code('ABCD-EFGH-IJKL')]));
    });

    // The cubit sits below the navigator that hosts the dialog, as it does in
    // the profile section, so the host has to carry it to whatever it opens.
    Widget buildHost() => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: BlocProvider<InvitationCodesCubit>.value(
          value: cubit,
          child: Scaffold(
            body: Builder(
              builder: (context) => ElevatedButton(
                onPressed: () => showInvitationCodes(context),
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      ),
    );

    Future<void> open(WidgetTester tester) async {
      await tester.pumpWidget(buildHost());
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
    }

    testWidgets('fills the screen on a phone', (tester) async {
      sizeView(tester, phoneViewSize);
      await open(tester);

      expect(find.byType(InvitationCodesModal), findsOneWidget);
      expectModalFillsViewport(tester, phoneViewSize);
      expect(find.text('ABCD-EFGH-IJKL'), findsOneWidget);
    });

    // A route push would cover the window whole, sidebar and window chrome
    // included, so desktop gets a card over what it came from instead.
    testWidgets('presents a card on desktop', (tester) async {
      sizeView(tester, desktopViewSize);
      await open(tester);

      expect(find.byType(InvitationCodesModal), findsOneWidget);
      expectModalIsCard(tester, desktopViewSize);
      // The codes the card lists are the ones the host handed it.
      expect(find.text('ABCD-EFGH-IJKL'), findsOneWidget);
    }, variant: desktopPlatform);

    // A desktop window narrow enough to sit in the small tier still gets the
    // card: nothing reserves the window's own chrome on a full-bleed surface.
    testWidgets('presents a card in a narrow desktop window', (tester) async {
      sizeView(tester, phoneViewSize);
      await open(tester);

      expectModalIsCard(tester, phoneViewSize);
    }, variant: desktopPlatform);
  });
}
