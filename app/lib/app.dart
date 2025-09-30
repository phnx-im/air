// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:permission_handler/permission_handler.dart';
import 'package:air/background_service.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/user/user.dart';
import 'package:air/util/interface_scale.dart';
import 'package:air/util/platform.dart';
import 'package:provider/provider.dart';

import 'chat_details/chat_details.dart';
import 'registration/registration.dart';
import 'theme/theme.dart';

final _log = Logger('App');

final _appRouter = AppRouter();

class App extends StatefulWidget {
  const App({super.key});

  @override
  State<App> createState() => _AppState();
}

class _AppState extends State<App> with WidgetsBindingObserver {
  final CoreClient _coreClient = CoreClient();
  final _backgroundService = BackgroundService();

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

    _requestNotificationPermissions();

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
          create:
              (context) => LoadableUserCubit(
                (_coreClient..loadDefaultUser()).userStream,
              ),
          lazy: false, // immediately try to load the user
        ),
        BlocProvider<UserSettingsCubit>(
          create: (context) => UserSettingsCubit(),
        ),
      ],
      child: InterfaceScale(
        child: MaterialApp.router(
          onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          debugShowCheckedModeBanner: false,
          theme: lightTheme,
          darkTheme: darkTheme,
          routerConfig: _appRouter,
          builder:
              (context, router) => LoadableUserCubitProvider(
                appStateController: _appStateController,
                child: ChatDetailsCubitProvider(child: router!),
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
            context.read<NavigationCubit>().openHome();
            context.read<UserSettingsCubit>().loadState(user: user);
          case LoadingUser() || LoadedUser(user: null):
            context.read<NavigationCubit>().openIntro();
            context.read<UserSettingsCubit>().reset();
        }
      },
      builder:
          (context, loadableUser) =>
              loadableUser.user == null
                  ? child
                  : MultiBlocProvider(
                    providers: [
                      // Logged-in user and contacts are accessible everywhere inside the app after
                      // the user is loaded.
                      BlocProvider<UserCubit>(
                        create:
                            (context) => UserCubit(
                              coreClient: context.read<CoreClient>(),
                              navigationCubit: context.read<NavigationCubit>(),
                              appStateStream: appStateController.stream,
                            ),
                      ),
                      BlocProvider<UsersCubit>(
                        create:
                            (context) => UsersCubit(
                              userCubit: context.read<UserCubit>(),
                            ),
                      ),
                    ],
                    child: RepositoryProvider<AttachmentsRepository>(
                      create:
                          (context) => AttachmentsRepository(
                            userCubit: context.read<UserCubit>().impl,
                          ),
                      lazy: false, // immediately download pending attachments
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

void _requestNotificationPermissions() async {
  if (Platform.isMacOS) {
    // macOS: Use custom method channel
    _log.info("Requesting notification permission for macOS");
    try {
      final granted = await requestNotificationPermission();
      _log.info("macOS notification permission granted: $granted");
    } on PlatformException catch (e) {
      _log.severe(
        "System error requesting macOS notification permission: ${e.message}",
      );
    }
  } else if (Platform.isAndroid || Platform.isIOS) {
    // Mobile: Use permission_handler
    var status = await Permission.notification.status;
    switch (status) {
      case PermissionStatus.denied:
        _log.info("Notification permission denied, will ask the user");
        var requestStatus = await Permission.notification.request();
        _log.fine("The status is $requestStatus");
        break;
      default:
        _log.info("Notification permission status: $status");
    }
  }
}

/// Creates a [ChatDetailsCubit] for the current chat
///
/// This is used to mount the chat details cubit when the user
/// navigates to a chat. The [ChatDetailsCubit] can be
/// then used from any screen.
class ChatDetailsCubitProvider extends StatelessWidget {
  const ChatDetailsCubitProvider({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return BlocBuilder<NavigationCubit, NavigationState>(
      buildWhen: (previous, current) => current.chatId != previous.chatId,
      builder: (context, state) {
        final chatId = state.chatId;
        if (chatId == null) {
          return child;
        }
        return BlocProvider(
          // rebuilds the cubit when a different chat is selected
          key: ValueKey("chat-details-cubit-$chatId"),
          create:
              (context) => ChatDetailsCubit(
                userCubit: context.read<UserCubit>(),
                chatId: chatId,
              ),
          child: child,
        );
      },
    );
  }
}
