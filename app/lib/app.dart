// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/material/scroll_behavior.dart';
import 'package:air/ds/material/theme_data.dart';
import 'package:air/features/navigation/app_router.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/features/user/user_session_cubit.dart';
import 'package:air/features/user/user_session_scope.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/l10n/language_options.dart';
import 'package:air/l10n/supported_locales.dart';
import 'package:air/platform/app_lifecycle_handler.dart';
import 'package:air/platform/background_service.dart';
import 'package:air/platform/method_channel.dart';
import 'package:air/platform/notifications.dart';
import 'package:air/share/pending_share.dart';
import 'package:air/util/interface_scale.dart';
import 'package:air/util/time/app_clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';
import 'package:system_date_time_format/system_date_time_format.dart';
import 'package:uuid/uuid.dart';

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

class _AppState extends State<App> {
  final CoreClient _coreClient = CoreClient();
  final _backgroundService = BackgroundService();
  late final _lifecycleHandler = AppLifecycleHandler(coreClient: _coreClient);
  final _log = Logger('App');

  final StreamController<ChatId> _openedNotificationController =
      StreamController<ChatId>();
  late final StreamSubscription<ChatId> _openedNotificationSubscription;
  final StreamController<ShareHandoff> _shareHandoffController =
      StreamController<ShareHandoff>();
  late final StreamSubscription<ShareHandoff> _shareHandoffSubscription;
  final NavigationCubit _navigationCubit = NavigationCubit(
    notificationContext: NotificationContextBase(
      notificationService: DartNotificationServiceExtension.create(),
    ),
  );
  final UserSettingsCubit _userSettingsCubit = UserSettingsCubit();
  final AppLocaleCubit _appLocaleCubit = AppLocaleCubit();

  @override
  void initState() {
    super.initState();
    _lifecycleHandler.start();

    initMethodChannel(
      _openedNotificationController.sink,
      _shareHandoffController.sink,
    );
    _openedNotificationSubscription = _openedNotificationController.stream
        .listen((chatId) {
          // Dismiss any active overlays before navigating to the chat
          _appRouter.dismissOverlays();
          _navigationCubit.openChat(chatId);
        });
    _shareHandoffSubscription = _shareHandoffController.stream.listen(
      _onShareHandoff,
    );

    // Fetch potential initial notification that launched the app on Android
    // cold start. The share handoff is consumed in `_loadInitialUser`.
    unawaited(consumeInitialNotification(_openedNotificationController.sink));

    _backgroundService.start(runImmediately: true);
  }

  /// Routes content the Android share activity handed over.
  void _onShareHandoff(ShareHandoff handoff) {
    if (_coreClient.maybeUser == null ||
        _navigationCubit.state.isCreatingAccount) {
      _log.info('Dropping a share handoff: no usable user is loaded');
      unawaited(handoff.share.deleteFiles());
      return;
    }
    _appRouter.dismissOverlays();
    unawaited(
      _navigationCubit.openShare(handoff.share, chatId: handoff.chatId),
    );
  }

  @override
  void dispose() {
    _lifecycleHandler.stop();
    _openedNotificationSubscription.cancel();
    _openedNotificationController.close();
    _shareHandoffSubscription.cancel();
    _shareHandoffController.close();
    _backgroundService.stop();
    _userSettingsCubit.close();
    _appLocaleCubit.close();
    super.dispose();
  }

  /// Loads the client record given on the command line, or the default user.
  Future<void> _loadInitialUser() async {
    final clientRecordId = widget.clientRecordId;
    try {
      if (clientRecordId == null) {
        await _coreClient.loadDefaultUser();
      } else {
        _log.info(
          "Loading client record from the command line: $clientRecordId",
        );
        await _coreClient.loadUser(clientRecordId: clientRecordId);
      }
    } catch (error) {
      _log.severe(
        "Error loading client record ${clientRecordId ?? 'default'}: $error",
      );
    }
    // When loading failed: the share is dropped and files are deleted.
    await consumeInitialShare(_shareHandoffController.sink);
  }

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      providers: [
        Provider.value(value: _coreClient),
        BlocProvider<NavigationCubit>.value(value: _navigationCubit),
        BlocProvider<UserSettingsCubit>.value(value: _userSettingsCubit),
        BlocProvider<AppLocaleCubit>.value(value: _appLocaleCubit),
        BlocProvider<RegistrationCubit>(
          create: (context) => RegistrationCubit(coreClient: _coreClient),
        ),
        BlocProvider<UserSessionCubit>(
          create: (_) {
            unawaited(_loadInitialUser());
            return UserSessionCubit(
              coreClient: _coreClient,
              navigationCubit: _navigationCubit,
              userSettingsCubit: _userSettingsCubit,
              appLocaleCubit: _appLocaleCubit,
            );
          },
          lazy: false, // immediately try to load the user
        ),
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
                // Prefer the persisted user locale over the in-memory one.
                final locale = localeFromTag(userLocaleCode) ?? appLocale;

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
                  builder: (context, router) => UserSessionScope(
                    appStateStream: _lifecycleHandler.appStateStream,
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
