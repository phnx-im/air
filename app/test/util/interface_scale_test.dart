// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/breakpoint.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/util/interface_scale.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../mocks.dart';

void main() {
  group('InterfaceScale', () {
    late MockUserSettingsCubit userSettingsCubit;
    late double layoutWidth;
    late Size mediaQuerySize;
    late TextScaler textScaler;

    setUp(() {
      userSettingsCubit = MockUserSettingsCubit();
    });

    Future<void> pumpSubject(WidgetTester tester, {double? userScale}) async {
      tester.view.physicalSize = const Size(768, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      when(() => userSettingsCubit.state)
          .thenReturn(UserSettings(interfaceScale: userScale));

      await tester.pumpWidget(
        BlocProvider<UserSettingsCubit>.value(
          value: userSettingsCubit,
          child: InterfaceScale(
            child: LayoutBuilder(
              builder: (context, constraints) {
                layoutWidth = constraints.maxWidth;
                mediaQuerySize = MediaQuery.sizeOf(context);
                textScaler = MediaQuery.textScalerOf(context);
                return const SizedBox.shrink();
              },
            ),
          ),
        ),
      );
    }

    testWidgets('leaves the media query alone at scale 1', (tester) async {
      await pumpSubject(tester);

      expect(mediaQuerySize, const Size(768, 800));
      expect(layoutWidth, 768);
    });

    testWidgets('reports the scaled size to the breakpoint', (tester) async {
      await pumpSubject(tester, userScale: 1.25);

      expect(layoutWidth, closeTo(614.4, 0.001));
      expect(mediaQuerySize.width, closeTo(layoutWidth, 0.001));
      expect(mediaQuerySize.height, closeTo(640, 0.001));
      expect(Breakpoint.fromWidth(mediaQuerySize.width), Breakpoint.small);
    });

    testWidgets('turns the system text scale into an interface scale', (
      tester,
    ) async {
      tester.platformDispatcher.textScaleFactorTestValue = 1.25;
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

      await pumpSubject(tester);

      expect(textScaler, TextScaler.noScaling);
      expect(layoutWidth, closeTo(614.4, 0.001));
      expect(mediaQuerySize.width, closeTo(layoutWidth, 0.001));
    }, variant: TargetPlatformVariant.only(TargetPlatform.linux));

    testWidgets(
      'keeps text unscaled when the user scale cancels the system scale',
      (tester) async {
        tester.platformDispatcher.textScaleFactorTestValue = 1.25;
        addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

        await pumpSubject(tester, userScale: 0.8);

        expect(textScaler, TextScaler.noScaling);
        expect(layoutWidth, 768);
        expect(mediaQuerySize.width, 768);
      },
      variant: TargetPlatformVariant.only(TargetPlatform.linux),
    );
  });
}
