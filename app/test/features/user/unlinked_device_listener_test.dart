// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/features/user/unlinked_device_listener.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../mocks.dart';

class MockCoreClient extends Mock implements CoreClient {}

void main() {
  testWidgets('tears down when the initial state is already unlinked', (
    tester,
  ) async {
    final userCubit = MockUserCubit();
    final coreClient = MockCoreClient();
    when(
      () => userCubit.state,
    ).thenReturn(MockUiUser(id: 1, accountUnlinked: true));
    when(() => coreClient.deleteCurrentDatabase()).thenAnswer((_) async {});

    await tester.pumpWidget(
      RepositoryProvider<CoreClient>.value(
        value: coreClient,
        child: BlocProvider<UserCubit>.value(
          value: userCubit,
          child: const UnlinkedDeviceHandler(child: SizedBox()),
        ),
      ),
    );
    await tester.pump();

    verify(() => coreClient.deleteCurrentDatabase()).called(1);
  });

  testWidgets('tears down when unlinking races listener initialization', (
    tester,
  ) async {
    final userCubit = MockUserCubit();
    final coreClient = MockCoreClient();
    final linked = MockUiUser(id: 1);
    final unlinked = MockUiUser(id: 1, accountUnlinked: true);
    var stateReads = 0;
    when(() => userCubit.state).thenAnswer((_) {
      stateReads++;
      return stateReads == 1 ? linked : unlinked;
    });
    when(() => userCubit.stream).thenAnswer((_) => Stream.value(unlinked));
    when(() => coreClient.deleteCurrentDatabase()).thenAnswer((_) async {});

    await tester.pumpWidget(
      RepositoryProvider<CoreClient>.value(
        value: coreClient,
        child: BlocProvider<UserCubit>.value(
          value: userCubit,
          child: const UnlinkedDeviceHandler(child: SizedBox()),
        ),
      ),
    );
    await tester.pump();

    verify(() => coreClient.deleteCurrentDatabase()).called(1);
  });
}
