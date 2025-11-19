// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:air/chat/chat_details.dart';
import 'package:air/chat/create_group_screen.dart';
import 'package:air/developer/developer.dart';
import 'package:air/home_screen.dart';
import 'package:air/intro_screen.dart';
import 'package:air/registration/registration.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';
import 'package:air/core/core.dart';

import 'navigation_cubit.dart';

final _log = Logger('AppRouter');

class EmptyConfig {
  const EmptyConfig();
}

class AppRouter implements RouterConfig<EmptyConfig> {
  AppRouter();

  final AppRouterDelegate _routerDelegate = AppRouterDelegate();

  final AppBackButtonDispatcher _backButtonDispatcher =
      AppBackButtonDispatcher();

  @override
  BackButtonDispatcher? get backButtonDispatcher => _backButtonDispatcher;

  @override
  RouteInformationParser<EmptyConfig>? get routeInformationParser => null;

  @override
  RouteInformationProvider? get routeInformationProvider => null;

  @override
  RouterDelegate<EmptyConfig> get routerDelegate => _routerDelegate;
}

/// The main application router
///
/// Builds pages from the navigation state [NavigationState] provided by the
/// [NavigationCubit]. This is where the translation from the navigation
/// state to the actual list of pages happens.
class AppRouterDelegate extends RouterDelegate<EmptyConfig> {
  AppRouterDelegate();

  final GlobalKey<NavigatorState> _navigatorKey = GlobalKey<NavigatorState>();

  final PageStorageBucket _bucket = PageStorageBucket();

  @override
  Widget build(BuildContext context) {
    final navigationState = context.watch<NavigationCubit>().state;

    // hide material banners if any
    ScaffoldMessenger.of(context).hideCurrentMaterialBanner();

    final screenType = context.responsiveScreenType;

    // routing
    final List<MaterialPage> pages = switch (navigationState) {
      NavigationState_Intro(:final screens) => [
        // The first screen is always the intro screen
        const MaterialPage(
          key: ValueKey("intro-screen"),
          canPop: false,
          child: IntroScreen(),
        ),
        for (final screenType in screens)
          MaterialPage(key: screenType.key, child: screenType.screen),
      ],
      NavigationState_Home(:final home) => home.pages(screenType),
    };

    _log.finer(
      "AppRouterDelegate.build: navigationState = $navigationState, pages=$pages",
    );

    return PageStorage(
      bucket: _bucket,
      child: Navigator(
        key: _navigatorKey,
        pages: pages,
        // Note: onPopPage is deprecated, and instead we should use
        // onDidRemovePage. However, the latter does not allow to distinguish
        // whether the page was popped by the user or programmatically.
        //
        // Also see
        //   * <https://github.com/phnx-im/air/issues/244>
        //   * <https://github.com/flutter/flutter/issues/109494>
        //
        // ignore: deprecated_member_use
        onPopPage: (route, result) {
          // check whether the page was popped by the back button
          if (!route.didPop(result)) {
            return false;
          }
          if (route.settings case MaterialPage _) {
            return context.read<NavigationCubit>().pop();
          }
          return false;
        },
      ),
    );
  }

  /// Back button handler
  @override
  Future<bool> popRoute() {
    // Pop the last route if it is pageless. See `isPageless`.
    var poppedPagelessRoute = false;
    var triedToPop = false;
    _navigatorKey.currentState?.popUntil((route) {
      if (triedToPop) {
        return true; // stop popping
      }
      triedToPop = true;
      poppedPagelessRoute = isPageless(route);
      return !poppedPagelessRoute; // pop if is pageless
    });

    return poppedPagelessRoute
        ? SynchronousFuture(true)
        : SynchronousFuture(
          _navigatorKey.currentContext?.read<NavigationCubit>().pop() ?? false,
        );
  }

  @override
  void addListener(VoidCallback listener) {
    // Listening to the navigation state is not supported.
  }

  @override
  void removeListener(VoidCallback listener) {
    // Listening to the navigation state is not supported.
  }

  @override
  Future<void> setNewRoutePath(EmptyConfig configuration) async {
    // This called in Web when an URL is entered in the browser, or when `Router.navigate` is called
    // programmatically. We dont handle these cases.
  }
}

class AppBackButtonDispatcher extends RootBackButtonDispatcher {}

/// Convert an [IntroScreenType] into a [ValueKey] and a screen [Widget].
extension on IntroScreenType {
  ValueKey<String> get key => switch (this) {
    IntroScreenType_SignUp() => const ValueKey("sign-up-screen"),
    IntroScreenType_UsernameOnboarding() => const ValueKey(
      "username-onboarding-screen",
    ),
    IntroScreenType_DeveloperSettings(field0: final screen) => ValueKey(
      "developer-settings-screen-$screen",
    ),
  };

  Widget get screen => switch (this) {
    IntroScreenType_SignUp() => const SignUpScreen(),
    IntroScreenType_UsernameOnboarding() => const UsernameOnboardingScreen(),
    IntroScreenType_DeveloperSettings(field0: final screen) => switch (screen) {
      DeveloperSettingsScreenType.root => const DeveloperSettingsScreen(),
      DeveloperSettingsScreenType.changeUser => const ChangeUserScreen(),
      DeveloperSettingsScreenType.logs => const LogsScreen(),
    },
  };
}

/// Convert [HomeNavigation] state into a list of pages.
extension on HomeNavigationState {
  ChatId? get openChatId => chatOpen ? chatId : null;

  List<MaterialPage> pages(ResponsiveScreenType screenType) {
    const homeScreenPage = NoAnimationPage(
      key: ValueKey("home-screen"),
      canPop: false,
      child: HomeScreen(),
    );
    return [
      homeScreenPage,
      if (createGroupOpen)
        const MaterialPage(
          key: ValueKey("create-group-screen"),
          child: CreateGroupScreen(),
        ),
      ...switch (userSettingsScreen) {
        null => [],
        UserSettingsScreenType.root => [
          const MaterialPage(
            key: ValueKey("user-settings-screen-root"),
            child: UserSettingsScreen(),
          ),
        ],
        UserSettingsScreenType.editDisplayName => [
          const MaterialPage(
            key: ValueKey("user-settings-screen-root"),
            child: UserSettingsScreen(),
          ),
          const MaterialPage(
            key: ValueKey("user-settings-screen-edit-display-name"),
            child: EditDisplayNameScreen(),
          ),
        ],
        UserSettingsScreenType.addUserHandle => [
          const MaterialPage(
            key: ValueKey("user-settings-screen-root"),
            child: UserSettingsScreen(),
          ),
          const MaterialPage(
            key: ValueKey("user-settings-screen-add-user-handle"),
            child: AddUserHandleScreen(),
          ),
        ],
        UserSettingsScreenType.help => [
          const MaterialPage(
            key: ValueKey("user-settings-screen-root"),
            child: UserSettingsScreen(),
          ),
          const MaterialPage(
            key: ValueKey("user-settings-screen-help"),
            child: HelpScreen(),
          ),
        ],
        UserSettingsScreenType.deleteAccount => [
          const MaterialPage(
            key: ValueKey("user-settings-screen-root"),
            child: UserSettingsScreen(),
          ),
          const MaterialPage(
            key: ValueKey("user-settings-screen-delete-account"),
            child: DeleteAccountScreen(),
          ),
        ],
      },
      if (openChatId != null && screenType == ResponsiveScreenType.mobile)
        const MaterialPage(key: ValueKey("chat-screen"), child: ChatScreen()),
      if (openChatId != null && chatDetailsOpen)
        const MaterialPage(
          key: ValueKey("chat-details-screen"),
          child: ChatDetailsScreen(),
        ),
      if (openChatId != null && chatDetailsOpen && groupMembersOpen)
        const MaterialPage(
          key: ValueKey("chat-group-members-screen"),
          child: GroupMembersScreen(),
        ),
      if (openChatId != null && memberDetails != null)
        const MaterialPage(
          key: ValueKey("chat-member-details-screen"),
          child: MemberDetailsScreen(),
        ),
      if (openChatId != null && chatDetailsOpen && addMembersOpen)
        const MaterialPage(
          key: ValueKey("add-members-screen"),
          child: AddMembersScreen(),
        ),
      ...switch (developerSettingsScreen) {
        null => [],
        DeveloperSettingsScreenType.root => [
          const MaterialPage(
            key: ValueKey("developer-settings-screen"),
            child: DeveloperSettingsScreen(),
          ),
        ],
        DeveloperSettingsScreenType.changeUser => [
          const MaterialPage(
            key: ValueKey("developer-settings-screen-root"),
            child: DeveloperSettingsScreen(),
          ),
          const MaterialPage(
            key: ValueKey("developer-settings-screen-change-user"),
            child: ChangeUserScreen(),
          ),
        ],
        DeveloperSettingsScreenType.logs => [
          const MaterialPage(
            key: ValueKey("developer-settings-screen-root"),
            child: DeveloperSettingsScreen(),
          ),
          const MaterialPage(
            key: ValueKey("developer-settings-screen-logs"),
            child: LogsScreen(),
          ),
        ],
      },
    ];
  }
}

class NoAnimationPage<T> extends MaterialPage<T> {
  const NoAnimationPage({
    super.name,
    super.canPop,
    required super.child,
    super.key,
  });

  @override
  Route<T> createRoute(BuildContext context) {
    return NoAnimationMaterialPageRoute<T>(
      settings: this,
      builder: (context) => child,
    );
  }
}

class NoAnimationMaterialPageRoute<T> extends MaterialPageRoute<T> {
  NoAnimationMaterialPageRoute({super.settings, required super.builder});

  @override
  Widget buildTransitions(
    BuildContext context,
    Animation<double> animation,
    Animation<double> secondaryAnimation,
    Widget child,
  ) {
    // return child without transition animation
    return child;
  }
}

/// A route which does not correspond to a page in the app.
///
/// Such a route is added by [`Navigator.push`] and does not correspond to a state in the
/// `NavigationState` inside `NavigationCubit`.
bool isPageless(Route<dynamic> route) => route.settings is! Page;
