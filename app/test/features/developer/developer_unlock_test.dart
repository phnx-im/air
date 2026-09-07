// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/developer/developer_unlock.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../mocks.dart';

/// Taps a run needs to unlock. The subject keeps its own copy private, nothing
/// outside the gesture should depend on the count.
const _unlockTaps = 9;

void main() {
  group('useDeveloperUnlock', () {
    late MockUserSettingsCubit userSettingsCubit;
    late MockNavigationCubit navigationCubit;
    late List<bool> continuedRun;

    setUp(() {
      userSettingsCubit = MockUserSettingsCubit();
      navigationCubit = MockNavigationCubit();
      continuedRun = [];

      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      when(
        () => userSettingsCubit.setDeveloperMode(value: any(named: 'value')),
      ).thenAnswer((_) async {});
      when(() => navigationCubit.openDeveloperSettings()).thenAnswer((_) {});
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        BlocProvider<NavigationCubit>.value(value: navigationCubit),
      ],
      child: _UnlockHost(onTapped: (value) => continuedRun.add(value)),
    );

    Future<void> tapTimes(WidgetTester tester, int times) async {
      for (var i = 0; i < times; i++) {
        await tester.tap(find.byType(_UnlockHost));
      }
      await tester.pump();
    }

    testWidgets('a full run unlocks the surface and opens it', (tester) async {
      await tester.pumpWidget(buildSubject());

      await tapTimes(tester, _unlockTaps);

      verify(() => userSettingsCubit.setDeveloperMode(value: true)).called(1);
      verify(() => navigationCubit.openDeveloperSettings()).called(1);
    });

    testWidgets('a run one tap short unlocks nothing', (tester) async {
      await tester.pumpWidget(buildSubject());

      await tapTimes(tester, _unlockTaps - 1);

      verifyNever(
        () => userSettingsCubit.setDeveloperMode(value: any(named: 'value')),
      );
      verifyNever(() => navigationCubit.openDeveloperSettings());
    });

    testWidgets('only the first tap leaves the host to speak', (tester) async {
      await tester.pumpWidget(buildSubject());

      await tapTimes(tester, 2);

      expect(continuedRun, [false, true]);
    });

    testWidgets('the run restarts after an unlock', (tester) async {
      await tester.pumpWidget(buildSubject());

      await tapTimes(tester, _unlockTaps + 1);

      expect(continuedRun.last, isFalse);
      verify(() => userSettingsCubit.setDeveloperMode(value: true)).called(1);
    });
  });
}

/// Hosts the hook and reports what each tap returned.
class _UnlockHost extends HookWidget {
  const _UnlockHost({required this.onTapped});

  final void Function(bool continuedRun) onTapped;

  @override
  Widget build(BuildContext context) {
    final unlock = useDeveloperUnlock();

    return GestureDetector(
      behavior: .opaque,
      onTap: () => onTapped(unlock()),
      child: const SizedBox(width: 100, height: 100),
    );
  }
}
