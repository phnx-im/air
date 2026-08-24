// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:air/platform/background_service.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/l10n/supported_locales.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/navigation/app_router.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/scroll_behavior.dart';
import 'package:air/ds/material/theme_data.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/unlinked_device_listener.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/share/share_targets.dart';
import 'package:air/util/interface_scale.dart';
import 'package:air/util/time/app_clock.dart';
import 'package:air/platform/notifications.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';
import 'package:system_date_time_format/system_date_time_format.dart';
import 'package:uuid/uuid.dart';
import 'package:air/features/onboarding/update_required_screen.dart';
import 'package:air/ds/patterns/nux/nux_scaffold_tokens.dart';
import 'package:air/features/chat/chats_repository.dart';

final _appRouter = AppRouter();

final scaffoldMessengerKey = GlobalKey<ScaffoldMessengerState>();

class App extends StatefulWidget {
  const App({super.key, this.clientRecordId});

  /// When set, this client record is opened at startup instead of the default
  /// one.
  final UuidValue? clientRecordId;

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
  final NavigationCubit _navigationCubit = NavigationCubit(
    notificationContext: NotificationContextBase(
      notificationService: DartNotificationServiceExtension.create(),
    ),
  );

  final StreamController<AppState> _appStateController =
      StreamController<AppState>.broadcast();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);

    initMethodChannel(_openedNotificationController.sink);
    _openedNotificationSubscription = _openedNotificationController.stream
        .listen((chatId) {
          // Dismiss any active overlays before navigating to the chat
          _appRouter.dismissOverlays();
          _navigationCubit.openChat(chatId);
        });

    // Fetch potential initial notification that launched the app on Android
    // cold start.
    unawaited(consumeInitialNotification(_openedNotificationController.sink));

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

    if (DeviceType.isDesktop && state == AppLifecycleState.inactive) {
      // On desktop platforms, the inactive state is entered when the user
      // switches to another app. In that case, we want to treat it as
      // background state.
      _appStateController.sink.add(AppState.desktopBackground);
      return;
    }
    if (DeviceType.isPhone && state == AppLifecycleState.paused) {
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
      unawaited(_coreClient.refreshPushToken());
    }
  }

  /// Loads the client record given on the command line, or the default user.
  void _loadInitialUser() {
    final clientRecordId = widget.clientRecordId;
    if (clientRecordId == null) {
      _coreClient.loadDefaultUser();
      return;
    }
    _log.info("Loading client record from the command line: $clientRecordId");
    _coreClient.loadUser(clientRecordId: clientRecordId).onError((
      error,
      stackTrace,
    ) {
      _log.severe("Error loading client record $clientRecordId: $error");
    });
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
          create: (context) {
            _loadInitialUser();
            return LoadableUserCubit(_coreClient.userStream);
          },
          lazy: false, // immediately try to load the user
        ),
        BlocProvider<UserSettingsCubit>(
          create: (context) => UserSettingsCubit(),
        ),
        BlocProvider<AppLocaleCubit>(create: (context) => AppLocaleCubit()),
      ],
      child: InterfaceScale(
        // SDTFScope exposes date & time formatting system preferences
        child: SDTFScope(
          // One clock for every live timestamp in the app.
          child: AppClock(
            child: Builder(
              builder: (context) {
                final userLocaleCode = context.select(
                  (UserSettingsCubit cubit) => cubit.state.locale,
                );
                final appLocale = context.select(
                  (AppLocaleCubit cubit) => cubit.state,
                );
                // Prefer the persisted user locale, then the in-memory one.
                final locale = userLocaleCode != null
                    ? Locale(userLocaleCode)
                    : appLocale;

                return MaterialApp.router(
                  scrollBehavior: const AppScrollBehavior(),
                  scaffoldMessengerKey: scaffoldMessengerKey,
                  onGenerateTitle: (context) =>
                      AppLocalizations.of(context).appTitle,
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  supportedLocales: supportedLocalesWithFallback(
                    AppLocalizations.supportedLocales,
                    const Locale('en', 'US'),
                  ),
                  locale: locale,
                  debugShowCheckedModeBanner: false,
                  theme: lightTheme,
                  darkTheme: darkTheme,
                  routerConfig: _appRouter,
                  builder: (context, router) => LoadableUserCubitProvider(
                    appStateController: _appStateController,
                    child: BlocListener<NavigationCubit, NavigationState>(
                      // Drop the keyboard focus whenever we navigate, e.g. leaving
                      // a chat's message composer to open the contact/chat
                      // details. Otherwise the composer's FocusNode keeps focus
                      // while sitting under the pushed screens, and on iOS the
                      // keyboard reappears when a pageless route on top (like the
                      // safety code screen) is popped, because Flutter restores
                      // focus to it.
                      //
                      // Only touch devices have a software keyboard, and on
                      // desktop we want the composer to keep its focus, so this
                      // is scoped to non-desktop. The listener already only fires
                      // when the navigation state actually changes.
                      listenWhen: (previous, current) => !DeviceType.isDesktop,
                      listener: (context, state) =>
                          FocusManager.instance.primaryFocus?.unfocus(),
                      child: router!,
                    ),
                  ),
                );
              },
            ),
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
      listener: (context, loadableUser) async {
        final navigationCubit = context.read<NavigationCubit>();
        final userSettingsCubit = context.read<UserSettingsCubit>();

        // Side Effect: navigate to the home screen or away to the intro
        // screen, depending on whether the user was loaded or unloaded.
        switch (loadableUser) {
          case LoadedUser(:final user):
            final coreClient = context.read<CoreClient>();
            final appLocaleCubit = context.read<AppLocaleCubit>();

            // Bind the settings cubit to the user before navigating anywhere.
            // The logged-in subtree below is gated on the attachment, because
            // it reads the user-bound settings cubit impl.
            await userSettingsCubit.attach(user: user);

            // Only navigate to home if not already there. A push
            // notification deep link may have already set the state to
            // Home with a specific chat open, so we must not overwrite it.
            // Account creation is left alone too: the user exists a step
            // before that flow is done, and it opens home itself.
            final navigationState = navigationCubit.state;
            if (navigationState is! HomeState &&
                !navigationState.isCreatingAccount) {
              navigationCubit.openHome();
            }
            final userLocaleCode = userSettingsCubit.state.locale;
            final appLocale = appLocaleCubit.state;
            if (userLocaleCode != null) {
              // Sync UI locale with persisted user preference.
              appLocaleCubit.setLocale(Locale(userLocaleCode));
            } else if (appLocale != null) {
              // Persist pre-user selection once the user exists.
              await userSettingsCubit.setLocale(value: appLocale.languageCode);
            }
            unawaited(coreClient.refreshPushToken());

          case UnloadingUser():
            final loadableUserCubit = context.read<LoadableUserCubit>();

            navigationCubit.openIntro();
            // The published share targets carry chat names and avatars, so
            // drop them before the account unloads.
            unawaited(clearShareTargets());
            userSettingsCubit.detach();

            // Fully unload the user to dispose all user related providers, but
            // only after enough time to finish the transition to the intro
            // screen.
            Future.delayed(const Duration(milliseconds: 500), () {
              loadableUserCubit.finishUnloading();
            });

          case LoadingUser() || UnloadedUser():
        }
      },
      builder: (context, loadableUser) {
        // The logged-in subtree reads the settings cubit's user-bound impl,
        // which exists only once the listener above has attached it. Until
        // then, keep showing the same screen as while the user is loading.
        final settingsAttached = context.select(
          (UserSettingsCubit cubit) => cubit.isAttached,
        );

        Widget resolved(Widget child) {
          unawaited(dismissSplashScreen());
          return child;
        }

        return switch (loadableUser) {
          // Neither the login state nor the destination is settled yet.
          // Stay on the splash instead of the intro screen: a returning
          // user would otherwise see its sign-up chrome flash before
          // landing on Home.
          LoadingUser() => const _LoadingSplash(),
          LoadedUser() when !settingsAttached => const _LoadingSplash(),
          UnloadedUser() => resolved(child),
          LoadedUser(:final user) || UnloadingUser(:final user) => resolved(
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
        };
      },
    );
  }

  /// Checks if [LoadableUser] was loaded or unloaded
  bool _isUserLoadedOrUnloaded(LoadableUser previous, LoadableUser current) {
    return (previous is LoadedUser ||
            current is LoadedUser ||
            current is UnloadedUser) &&
        previous != current;
  }
}

/// Shown while the initial user load is in flight, in place of [IntroScreen].
class _LoadingSplash extends StatelessWidget {
  const _LoadingSplash();

  @override
  Widget build(BuildContext context) {
    return ColoredBox(color: NuxScaffoldTokens.surface(context));
  }
}
