// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:logging/logging.dart';
import 'package:air/features/chat_details/chat_details_modal.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/chat_details/create_group_modal.dart';
import 'package:air/features/developer/developer_settings_section.dart';
import 'package:air/features/home/home_screen.dart';
import 'package:air/features/onboarding/account_creation_flow.dart';
import 'package:air/features/onboarding/intro_screen.dart';
import 'package:air/features/onboarding/multi_device_provision_screen.dart';
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal_page.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/features/you/you_screen.dart';
import 'package:air/core/core.dart';

import 'package:air/features/navigation/navigation_cubit.dart';

final _log = Logger('AppRouter');

class EmptyConfig {
  const EmptyConfig();
}

class AppRouter implements RouterConfig<EmptyConfig> {
  AppRouter();

  final AppRouterDelegate _routerDelegate = AppRouterDelegate();

  final AppBackButtonDispatcher _backButtonDispatcher =
      AppBackButtonDispatcher();

  /// Dismiss any active overlays (pageless routes, context menus).
  ///
  /// Called before navigating from a push notification to ensure modals
  /// and dialogs don't remain on top of the new destination.
  void dismissOverlays() {
    _routerDelegate.dismissOverlays();
  }

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

    final breakpoint = context.breakpoint;

    // routing
    final List<Page> pages = switch (navigationState) {
      IntroState(:final screens) => [
        // The first screen is always the intro screen
        const MaterialPage(
          key: ValueKey("intro-screen"),
          canPop: false,
          child: IntroScreen(),
        ),
        for (final screenType in screens) screenType.page,
      ],
      HomeState(:final home) => home.pages(
        breakpoint: breakpoint,
        modalsFullBleed: ModalShellTokens.isFullBleed(context),
      ),
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
          if (route.settings case Page _) {
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
            _navigatorKey.currentContext?.read<NavigationCubit>().pop() ??
                false,
          );
  }

  /// Pop all pageless routes from the navigator.
  void dismissOverlays() {
    _navigatorKey.currentState?.popUntil((route) => !isPageless(route));
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

/// Convert an [IntroScreenType] into the [Page] that presents its screen.
extension on IntroScreenType {
  ValueKey<String> get key => switch (this) {
    IntroScreenType.accountCreation => const ValueKey(
      "account-creation-screen",
    ),
    IntroScreenType.linking => const ValueKey("linking-existing-device-screen"),
    IntroScreenType.developerSettings => const ValueKey(
      "developer-settings-screen",
    ),
  };

  Widget get screen => switch (this) {
    IntroScreenType.accountCreation => const AccountCreationFlow(),
    IntroScreenType.linking => const MultiDeviceProvisionScreen(),
    IntroScreenType.developerSettings => const DeveloperSettingsScreen(),
  };

  Page get page => switch (this) {
    IntroScreenType.accountCreation => ModalPage(key: key, child: screen),
    IntroScreenType.linking => MaterialPage(key: key, child: screen),
    IntroScreenType.developerSettings => MaterialPage(key: key, child: screen),
  };
}

/// Convert [HomeNavigation] state into a list of pages.
extension on HomeNavigationState {
  ChatId? get openChatId => chatOpen ? chatId : null;

  List<Page> pages({
    required Breakpoint breakpoint,
    required bool modalsFullBleed,
  }) {
    const homeScreenPage = NoAnimationPage(
      key: ValueKey("home-screen"),
      canPop: false,
      child: HomeScreen(),
    );
    final openChatId = this.openChatId;
    return [
      homeScreenPage,
      if (createGroupOpen)
        const ModalPage(
          key: ValueKey("create-group-modal"),
          child: CreateGroupModal(),
        ),
      // The profile is rendered inline inside HomeScreen on both layouts: as
      // the tab's own screen at the small breakpoint, and as the two panes of
      // the desktop layout above it. Only the phone pushes a section, which
      // the two-pane layout shows beside its list instead.
      if ((activeTab, youSection) case (
        HomeTab.profile,
        final section?,
      ) when breakpoint.isSmall)
        MaterialPage(
          key: ValueKey("you-section-screen-$section"),
          child: YouSectionScreen(section: section),
        ),
      if (openChatId != null && breakpoint.isSmall)
        const MaterialPage(key: ValueKey("chat-screen"), child: ChatScreen()),
      if (openChatId != null)
        ..._chatDetailsPages(openChatId, fullBleed: modalsFullBleed),
      if (shareDestinationOpen)
        const MaterialPage(
          key: ValueKey("share-destination-screen"),
          child: ChatListView(scaffold: true, shareMode: true),
        ),
    ];
  }

  List<Page> _chatDetailsPages(ChatId chatId, {required bool fullBleed}) {
    if (chatDetails.isEmpty) return const [];

    if (!fullBleed) {
      return [
        ModalPage(
          key: const ValueKey("chat-details-modal"),
          child: ChatDetailsModal(chatId: chatId, pages: chatDetails),
        ),
      ];
    }

    return [
      for (final page in chatDetails)
        ModalPage(
          key: page.paneKey,
          child: ChatDetailsModalScreen(chatId: chatId, page: page),
        ),
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
