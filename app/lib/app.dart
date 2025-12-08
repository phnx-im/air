// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/background_service.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/registration/registration.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/util/interface_scale.dart';
import 'package:air/util/platform.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

final _appRouter = AppRouter();

final scaffoldMessengerKey = GlobalKey<ScaffoldMessengerState>();

class App extends StatefulWidget {
  const App({super.key});

  @override
  State<App> createState() => _AppState();
}

class _AppState extends State<App> with WidgetsBindingObserver {
  final CoreClient _coreClient = CoreClient();
  final _backgroundService = BackgroundService();
  int? _backgroundTaskId;
  final _log = Logger('AppLifecycle');

  final StreamController<ChatId> _openedNotificationController =
      StreamController<ChatId>();
  late final StreamSubscription<ChatId> _openedNotificationSubscription;
  final NavigationCubit _navigationCubit = NavigationCubit();

  final StreamController<AppState> _appStateController =
      StreamController<AppState>.broadcast();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);

    initMethodChannel(_openedNotificationController.sink);
    _openedNotificationSubscription = _openedNotificationController.stream
        .listen((chatId) {
          _navigationCubit.openChat(chatId);
        });

    _backgroundService.start(runImmediately: true);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _openedNotificationSubscription.cancel();
    _openedNotificationController.close();
    _backgroundService.stop();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    super.didChangeAppLifecycleState(state);
    _onStateChanged(state);
  }

  Future<void> _onStateChanged(AppLifecycleState state) async {
    // Detect background transitions

    if (isPointer() && state == AppLifecycleState.inactive) {
      // On desktop platforms, the inactive state is entered when the user
      // switches to another app. In that case, we want to treat it as
      // background state.
      _appStateController.sink.add(AppState.desktopBackground);
      return;
    }
    if (isTouch() && state == AppLifecycleState.paused) {
      // On mobile platforms, the paused state is entered when the app
      // is closed. In that case, we want to treat it as background state.
      _appStateController.sink.add(AppState.mobileBackground);

      // iOS only
      if (Platform.isIOS) {
        // Request additional background time until the outbound service is
        // stopped
        await _prepareForBackground();
        // only set the badge count if the user is logged in
        if (_coreClient.maybeUser case final user?) {
          final count = await user.globalUnreadMessagesCount;
          await setBadgeCount(count);
        }
      }
      return;
    }

    // Detect foreground transitions

    if (state == AppLifecycleState.resumed) {
      _appStateController.sink.add(AppState.foreground);
    }
  }

  Future<void> _prepareForBackground() async {
    if (!Platform.isIOS) return;

    final startedAt = DateTime.now();
    _log.info('prepareForBackground: requesting background task');
    _backgroundTaskId = await beginBackgroundTask();
    _log.info(
      'prepareForBackground: background task started id=$_backgroundTaskId',
    );

    // Ask the coreclient to stop the outbound service gracefully
    final user = _coreClient.maybeUser;
    if (user == null) {
      _log.info('prepareForBackground: no user, ending background task');
      await endBackgroundTask(_backgroundTaskId);
      _backgroundTaskId = null;
      return;
    }

    try {
      await user.prepareForBackground();
    } finally {
      final elapsed = DateTime.now().difference(startedAt);
      await endBackgroundTask(_backgroundTaskId);
      _log.info(
        'prepareForBackground: ended background task after ${elapsed.inMilliseconds}ms',
      );
      _backgroundTaskId = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      providers: [
        Provider.value(value: _coreClient),
        BlocProvider<NavigationCubit>.value(value: _navigationCubit),
        BlocProvider<RegistrationCubit>(
          create: (context) => RegistrationCubit(coreClient: _coreClient),
        ),
        BlocProvider<LoadableUserCubit>(
          // loads the user on startup
          create: (context) =>
              LoadableUserCubit((_coreClient..loadDefaultUser()).userStream),
          lazy: false, // immediately try to load the user
        ),
        BlocProvider<UserSettingsCubit>(
          create: (context) => UserSettingsCubit(),
        ),
      ],
      child: InterfaceScale(
        child: MaterialApp.router(
          scaffoldMessengerKey: scaffoldMessengerKey,
          onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          debugShowCheckedModeBanner: false,
          theme: lightTheme,
          darkTheme: darkTheme,
          routerConfig: _appRouter,
          builder: (context, router) => LoadableUserCubitProvider(
            appStateController: _appStateController,
            child: router!,
          ),
        ),
      ),
    );
  }
}

class LoadableUserCubitProvider extends StatelessWidget {
  const LoadableUserCubitProvider({
    required this.appStateController,
    required this.child,
    super.key,
  });

  final StreamController<AppState> appStateController;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    // This bloc has two tasks:
    // 1. Listen to the loadable user and switch the navigation accordingly.
    // 2. Provide the logged in user to the app, when it is loaded.
    return BlocConsumer<LoadableUserCubit, LoadableUser>(
      listenWhen: _isUserLoadedOrUnloaded,
      buildWhen: _isUserLoadedOrUnloaded,
      listener: (context, loadableUser) {
        // Side Effect: navigate to the home screen or away to the intro
        // screen, depending on whether the user was loaded or unloaded.
        switch (loadableUser) {
          case LoadedUser(user: final user?):
            final registrationState = context.read<RegistrationCubit>().state;
            if (registrationState.needsUsernameOnboarding) {
              context.read<NavigationCubit>().openIntroScreen(
                const IntroScreenType.usernameOnboarding(),
              );
            } else {
              context.read<NavigationCubit>().openHome();
            }
            context.read<UserSettingsCubit>().loadState(user: user);
          case LoadingUser() || LoadedUser(user: null):
            context.read<NavigationCubit>().openIntro();
            context.read<UserSettingsCubit>().reset();
        }
      },
      builder: (context, loadableUser) => loadableUser.user == null
          ? child
          : MultiBlocProvider(
              providers: [
                // Logged-in user and contacts are accessible everywhere inside the app after
                // the user is loaded.
                BlocProvider<UserCubit>(
                  create: (context) => UserCubit(
                    coreClient: context.read<CoreClient>(),
                    navigationCubit: context.read<NavigationCubit>(),
                    appStateStream: appStateController.stream,
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
                    create: (context) => ChatsRepository(
                      userCubit: context.read<UserCubit>().impl,
                    ),
                    // immediately cache chats
                    lazy: false,
                  ),
                ],
                child: child,
              ),
            ),
    );
  }
}

/// Checks if [LoadableUser.user] transitioned from loaded to null or vice versa
bool _isUserLoadedOrUnloaded(LoadableUser previous, LoadableUser current) =>
    (previous.user != null || current.user != null) &&
    previous.user != current.user;
