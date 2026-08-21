// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/nux/nux_scaffold_tokens.dart';
import 'package:air/features/chat/chats_repository.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/update_required_screen.dart';
import 'package:air/features/user/unlinked_device_listener.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_session_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class UserSessionScope extends StatelessWidget {
  const UserSessionScope({
    required this.appStateStream,
    required this.child,
    super.key,
  });

  final Stream<AppState> appStateStream;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return BlocBuilder<UserSessionCubit, UserSessionState>(
      builder: (context, session) {
        Widget resolved(Widget child) {
          unawaited(dismissSplashScreen());
          return child;
        }

        return switch (session) {
          // Loaded or held alive during the logout grace period
          UserSessionState(:final user?) => resolved(
            KeyedSubtree(
              key: ValueKey(user.clientRecordId),
              child: MultiBlocProvider(
                providers: [
                  // Logged-in user and contacts are accessible everywhere
                  // inside the app after the user is loaded.
                  BlocProvider<UserCubit>(
                    create: (context) => UserCubit(
                      user: user,
                      navigationCubit: context.read<NavigationCubit>(),
                      appStateStream: appStateStream,
                    ),
                  ),
                  BlocProvider<UsersCubit>(
                    create: (context) =>
                        UsersCubit(userCubit: context.read<UserCubit>()),
                  ),
                ],
                child: MultiRepositoryProvider(
                  providers: [
                    RepositoryProvider<AttachmentsRepository>(
                      create: (context) => AttachmentsRepository(
                        userCubit: context.read<UserCubit>().impl,
                      ),
                      // immediately download pending attachments
                      lazy: false,
                    ),
                    RepositoryProvider<ChatsRepository>(
                      create: (context) => RustChatsRepository(
                        userCubit: context.read<UserCubit>().impl,
                      ),
                      dispose: (repository) => unawaited(repository.dispose()),
                      // immediately hydrate chats
                      lazy: false,
                    ),
                  ],
                  child: UnlinkedDeviceHandler(
                    child: UpdateRequiredScreen(child: child),
                  ),
                ),
              ),
            ),
          ),
          // Settled with no user => intro/registration flow
          UserSessionState(loggedOut: true) => resolved(child),
          // Neither login state nor destination settled => show splash screen
          _ => const _LoadingSplash(),
        };
      },
    );
  }
}

/// Shown while the initial user load is in flight, in place of [IntroScreen].
///
/// Avoids flickering the splash screen while the user is loading.
class _LoadingSplash extends StatelessWidget {
  const _LoadingSplash();

  @override
  Widget build(BuildContext context) {
    return ColoredBox(color: NuxScaffoldTokens.surface(context));
  }
}
