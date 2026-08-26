// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/chat_screen.dart';
import 'package:air/features/chat/chats_repository.dart' as chats_repository;
import 'package:air/features/chat_list/chat_list_view.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/features/message_list/message_list_cubit.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/navigation/app_tab_bar.dart';
import 'package:air/features/home/home_screen.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:device_frame/device_frame.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:system_date_time_format/system_date_time_format.dart';

import '../helpers.dart';
import '../features/message_list/message_list_test.dart';
import '../mocks.dart';
import 'content.dart';
import 'product_shot.dart';
import 'product_shot_device.dart';

/// Override with `--dart-define=GOLDENS_DIR=path/to/dir`.
const _goldensDir = String.fromEnvironment(
  'GOLDENS_DIR',
  defaultValue: 'goldens',
);

String _golden(String name) => '$_goldensDir/$name';

const androidPhysicalSize = Size(2160, 3840);
const iosPhysicalSize = Size(1290, 2796);
// One of the sizes accepted by the Mac App Store (16:10 retina).
const macosPhysicalSize = Size(2880, 1800);

class ProductShotInfo {
  ProductShotInfo({required this.hostPlatform, required this.targetPlatform});

  String hostPlatform;
  TargetPlatform targetPlatform;
}

void main() {
  setUpAll(() {
    registerFallbackValue(0.messageId());
    registerFallbackValue(0.userId());
    registerFallbackValue(0.attachmentId());
  });

  group('Chat List Product Shots', () {
    final backgroundColor = Primitive.neutral(NeutralShade.s100);
    final titleColor = Primitive.neutral(NeutralShade.s800);
    final subtitleColor = Primitive.neutral(NeutralShade.s600);
    final frameColor = Primitive.neutral(NeutralShade.s300);
    const title = 'Secure messaging\nfor everyone.';
    const subtitle = 'Everything in Air is\nend-to-end encrypted.';

    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(
        () => navigationCubit.state,
      ).thenReturn(const NavigationState.home());
      when(() => userCubit.state).thenReturn(MockUiUser(id: 10));
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles, defaultUserId: ownId),
      );
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(experimentalFeatures: false));
    });

    Widget buildSubject(ProductShotDevice device) => MultiRepositoryProvider(
      providers: [
        RepositoryProvider<AttachmentsRepository>.value(
          value: MockAttachmentsRepository(),
        ),
        RepositoryProvider<chats_repository.ChatsRepository>.value(
          value: FakeChatsRepository(chats),
        ),
      ],
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              final shot = ProductShot(
                backgroundColor: backgroundColor,
                titleColor: titleColor,
                subtitleColor: subtitleColor,
                title: title,
                subtitle: subtitle,
                frameColor: frameColor,
                device: device,
                child: const Stack(
                  children: [
                    Positioned.fill(child: ChatListView(scaffold: true)),
                    Positioned(
                      left: 0,
                      right: 0,
                      bottom: 0,
                      child: AppTabBar(),
                    ),
                  ],
                ),
              );

              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testLightTheme,
                themeMode: .light,
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: Material(
                  child: MediaQuery(
                    data: MediaQuery.of(
                      context,
                    ).copyWith(platformBrightness: .light),
                    child: shot,
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );

    Widget buildFramelessSubject(DeviceInfo device) => MultiRepositoryProvider(
      providers: [
        RepositoryProvider<AttachmentsRepository>.value(
          value: MockAttachmentsRepository(),
        ),
        RepositoryProvider<chats_repository.ChatsRepository>.value(
          value: FakeChatsRepository(chats),
        ),
      ],
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              final shot = DeviceFrame(
                device: device,
                screen: const Stack(
                  children: [
                    Positioned.fill(child: ChatListView(scaffold: true)),
                    Positioned(
                      left: 0,
                      right: 0,
                      bottom: 0,
                      child: AppTabBar(),
                    ),
                  ],
                ),
              );

              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testLightTheme,
                themeMode: .light,
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: Material(
                  color: Colors.transparent,
                  child: MediaQuery(
                    data: MediaQuery.of(
                      context,
                    ).copyWith(platformBrightness: .light),
                    child: Center(child: shot),
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );

    final productShotsMatrix = [
      ProductShotInfo(
        hostPlatform: 'macos',
        targetPlatform: TargetPlatform.iOS,
      ),
      ProductShotInfo(
        hostPlatform: 'macos',
        targetPlatform: TargetPlatform.macOS,
      ),
      ProductShotInfo(
        hostPlatform: 'linux',
        targetPlatform: TargetPlatform.android,
      ),
      ProductShotInfo(
        hostPlatform: 'linux',
        targetPlatform: TargetPlatform.linux,
      ),
      ProductShotInfo(
        hostPlatform: 'windows',
        targetPlatform: TargetPlatform.windows,
      ),
    ];

    for (final productShot in productShotsMatrix) {
      final device = productShot.targetPlatform.device;
      final deviceInfo = device.deviceInfo;
      final identifier = productShot.targetPlatform.identifier;

      // Build the product shot with marketing
      testProductShot(
        "Chat List (${deviceInfo.name})",
        hostPlatform: productShot.hostPlatform,
        // physicalSize: productShot.targetPlatform.device.screenSize,
        targetPlatform: productShot.targetPlatform,
        (tester) async {
          await tester.pumpWidget(buildSubject(device));
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(ProductShot),
            // Do not change the ios/android file names, as they are
            // referenced in stores/ios/en-US/screenshots and
            // stores/android/metadata/en-US/images/phone-screenshots
            matchesGoldenFile(_golden("chat_list.$identifier.png")),
          );
        },
      );

      // Build the product shot without marketing chrome
      testProductShot(
        "Chat List (${productShot.targetPlatform}, ${deviceInfo.name})",
        hostPlatform: productShot.hostPlatform,
        // physicalSize: iosPhysicalSize,
        targetPlatform: productShot.targetPlatform,
        (tester) async {
          await tester.pumpWidget(buildFramelessSubject(deviceInfo));
          await _precacheImages(tester);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(DeviceFrame),
            matchesGoldenFile(_golden("chat_list.$identifier.frameless.png")),
          );
        },
      );
    }

    // testProductShot(
    //   "Chat List (Android)",
    //   hostPlatform: 'linux',
    //   physicalSize: androidPhysicalSize,
    //   targetPlatform: TargetPlatform.android,
    //   (tester) async {
    //     await tester.pumpWidget(buildSubject(ProductShotPlatform.android));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/android/metadata/en-US/images/phone-screenshots
    //       matchesGoldenFile(_golden("chat_list.android.png")),
    //     );
    //   },
    // );

    // -- Product shots without marketing chrome --

    // final iosDeviceInfo = _deviceFrameFor(ProductShotPlatform.ios);
    // final androidDeviceInfo = _deviceFrameFor(ProductShotPlatform.android);
    // final macosDeviceInfo = _deviceFrameFor(ProductShotPlatform.macos);
    // final linuxDeviceInfo = _deviceFrameFor(ProductShotPlatform.linux);
    // final windowsDeviceInfo = _deviceFrameFor(ProductShotPlatform.windows);

    // testProductShot(
    //   "Chat List (iOS, ${iosDeviceInfo.name})",
    //   hostPlatform: 'macos',
    //   physicalSize: iosDeviceInfo.frameSize,
    //   targetPlatform: TargetPlatform.iOS,
    //   (tester) async {
    //     await tester.pumpWidget(buildFramelessSubject(iosDeviceInfo));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("chat_list", ProductShotPlatform.ios),
    //       ),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Chat List (Android, ${androidDeviceInfo.name})",
    //   hostPlatform: 'linux',
    //   physicalSize: androidDeviceInfo.frameSize,
    //   targetPlatform: TargetPlatform.android,
    //   (tester) async {
    //     await tester.pumpWidget(buildFramelessSubject(androidDeviceInfo));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("chat_list", ProductShotPlatform.android),
    //       ),
    //     );
    //   },
    // );
  });

  group("Private Chat", () {
    final backgroundColor = Primitive.chromatic(Hue.orange, Shade.s50);
    final titleColor = Primitive.chromatic(Hue.orange, Shade.s800);
    final subtitleColor = Primitive.chromatic(Hue.orange, Shade.s600);
    final frameColor = Primitive.chromatic(Hue.orange, Shade.s300);
    const title = 'Connect with friends.';
    const subtitle = 'Send messages in private chats.';

    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late MockAttachmentsRepository attachmentsRepository;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();
      attachmentsRepository = MockAttachmentsRepository();

      final chat = privateChat;

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
      );
      when(() => userCubit.state).thenReturn(MockUiUser(id: ownIdx));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chat, members: privateChatMembers));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      messageListCubit.setState(privateChatMessages);
      _stubAttachments(attachmentsRepository, privateChatAttachmentImages);
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: "attachmentId"),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));
    });

    Widget buildSubject(TargetPlatform platform) =>
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
          child: MultiBlocProvider(
            providers: [
              BlocProvider<NavigationCubit>.value(value: navigationCubit),
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<UsersCubit>.value(value: contactsCubit),
              BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
              BlocProvider<MessageListCubit>.value(value: messageListCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: Builder(
              builder: (context) {
                // final shotSize = _productShotSizeFor(platform);
                final shot = ProductShot(
                  // size: shotSize,
                  backgroundColor: backgroundColor,
                  titleColor: titleColor,
                  subtitleColor: subtitleColor,
                  title: title,
                  subtitle: subtitle,
                  frameColor: frameColor,
                  device: platform.device,
                  child: const ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                );

                return MaterialApp(
                  debugShowCheckedModeBanner: false,
                  theme: testLightTheme,
                  themeMode: ThemeMode.light,
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  home: Material(
                    child: MediaQuery(
                      data: MediaQuery.of(
                        context,
                      ).copyWith(platformBrightness: .light),
                      child: shot,
                    ),
                  ),
                );
              },
            ),
          ),
        );

    Widget buildFramelessSubject(DeviceInfo device) =>
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
          child: MultiBlocProvider(
            providers: [
              BlocProvider<NavigationCubit>.value(value: navigationCubit),
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<UsersCubit>.value(value: contactsCubit),
              BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
              BlocProvider<MessageListCubit>.value(value: messageListCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: Builder(
              builder: (context) {
                final shot = DeviceFrame(
                  device: device,
                  screen: const ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                );

                return MaterialApp(
                  debugShowCheckedModeBanner: false,
                  theme: testLightTheme,
                  themeMode: ThemeMode.light,
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  home: Material(
                    color: Colors.transparent,
                    child: MediaQuery(
                      data: MediaQuery.of(
                        context,
                      ).copyWith(platformBrightness: .light),
                      child: Center(child: shot),
                    ),
                  ),
                );
              },
            ),
          ),
        );

    // testProductShot(
    //   "Private Chat (iOS)",
    //   hostPlatform: "macos",
    //   physicalSize: iosPhysicalSize,
    //   targetPlatform: TargetPlatform.iOS,
    //   (tester) async {
    //     await tester.pumpWidget(buildSubject(ProductShotPlatform.ios));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/ios/en-US/screenshots
    //       matchesGoldenFile(_golden("private_chat.ios.png")),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Private Chat (Android)",
    //   hostPlatform: "linux",
    //   physicalSize: androidPhysicalSize,
    //   targetPlatform: TargetPlatform.android,
    //   (tester) async {
    //     await tester.pumpWidget(buildSubject(ProductShotPlatform.android));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/android/metadata/en-US/screenshots
    //       matchesGoldenFile(_golden("private_chat.android.png")),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Private Chat (${_deviceFrameFor(ProductShotPlatform.ios).name})",
    //   hostPlatform: "macos",
    //   physicalSize: _deviceFrameFor(ProductShotPlatform.ios).frameSize,
    //   targetPlatform: TargetPlatform.iOS,
    //   (tester) async {
    //     await tester.pumpWidget(buildFramelessSubject(ProductShotPlatform.ios));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("private_chat", ProductShotPlatform.ios),
    //       ),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Private Chat (${_deviceFrameFor(ProductShotPlatform.android).name})",
    //   hostPlatform: "linux",
    //   physicalSize: _deviceFrameFor(ProductShotPlatform.android).frameSize,
    //   targetPlatform: TargetPlatform.android,
    //   (tester) async {
    //     await tester.pumpWidget(
    //       buildFramelessSubject(ProductShotPlatform.android),
    //     );
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("private_chat", ProductShotPlatform.android),
    //       ),
    //     );
    //   },
    // );
  });

  group("Group Chat", () {
    final backgroundColor = Primitive.chromatic(Hue.blue, Shade.s50);
    final titleColor = Primitive.chromatic(Hue.blue, Shade.s800);
    final subtitleColor = Primitive.chromatic(Hue.blue, Shade.s600);
    final frameColor = Primitive.chromatic(Hue.blue, Shade.s300);
    const title = 'Create group chats.';
    const subtitle = 'Message with multiple people.';

    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late MockAttachmentsRepository attachmentsRepository;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();
      attachmentsRepository = MockAttachmentsRepository();

      final chat = groupChat;

      when(() => navigationCubit.state).thenReturn(
        NavigationState.home(home: HomeNavigationState(chatId: chat.id)),
      );
      when(() => userCubit.state).thenReturn(MockUiUser(id: ownIdx));
      when(
        () => contactsCubit.state,
      ).thenReturn(MockUsersState(profiles: userProfiles));
      when(
        () => chatDetailsCubit.state,
      ).thenReturn(ChatDetailsState(chat: chat, members: groupChatMembers));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
      messageListCubit.setState(groupChatMessages);
      _stubAttachments(attachmentsRepository, groupChatAttachmentImages);
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: "attachmentId"),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));
    });

    Widget buildSubject(ProductShotDevice device) =>
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
          child: MultiBlocProvider(
            providers: [
              BlocProvider<NavigationCubit>.value(value: navigationCubit),
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<UsersCubit>.value(value: contactsCubit),
              BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
              BlocProvider<MessageListCubit>.value(value: messageListCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: Builder(
              builder: (context) {
                final shot = ProductShot(
                  backgroundColor: backgroundColor,
                  titleColor: titleColor,
                  subtitleColor: subtitleColor,
                  title: title,
                  subtitle: subtitle,
                  frameColor: frameColor,
                  device: device,
                  child: const ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                );

                return MaterialApp(
                  debugShowCheckedModeBanner: false,
                  theme: testLightTheme,
                  themeMode: ThemeMode.light,
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  home: Material(
                    child: MediaQuery(
                      data: MediaQuery.of(
                        context,
                      ).copyWith(platformBrightness: .light),
                      child: shot,
                    ),
                  ),
                );
              },
            ),
          ),
        );

    Widget buildFramelessSubject(DeviceInfo device) =>
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
          child: MultiBlocProvider(
            providers: [
              BlocProvider<NavigationCubit>.value(value: navigationCubit),
              BlocProvider<UserCubit>.value(value: userCubit),
              BlocProvider<UsersCubit>.value(value: contactsCubit),
              BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
              BlocProvider<MessageListCubit>.value(value: messageListCubit),
              BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
            ],
            child: Builder(
              builder: (context) {
                final shot = DeviceFrame(
                  device: device,
                  screen: const ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                );

                return MaterialApp(
                  debugShowCheckedModeBanner: false,
                  theme: testLightTheme,
                  themeMode: ThemeMode.light,
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  home: Material(
                    color: Colors.transparent,
                    child: MediaQuery(
                      data: MediaQuery.of(
                        context,
                      ).copyWith(platformBrightness: .light),
                      child: Center(child: shot),
                    ),
                  ),
                );
              },
            ),
          ),
        );

    // testProductShot(
    //   "Group Chat (iOS)",
    //   hostPlatform: "macos",
    //   physicalSize: iosPhysicalSize,
    //   targetPlatform: TargetPlatform.iOS,
    //   (tester) async {
    //     await tester.pumpWidget(buildSubject(ProductShotPlatform.ios));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/ios/en-US/screenshots
    //       matchesGoldenFile(_golden("group_chat.ios.png")),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Group Chat (Android)",
    //   hostPlatform: "linux",
    //   physicalSize: androidPhysicalSize,
    //   targetPlatform: TargetPlatform.android,
    //   (tester) async {
    //     await tester.pumpWidget(buildSubject(ProductShotPlatform.android));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/android/metadata/en-US/screenshots
    //       matchesGoldenFile(_golden("group_chat.android.png")),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Group Chat (${_deviceFrameFor(ProductShotPlatform.ios).name})",
    //   hostPlatform: "macos",
    //   physicalSize: _deviceFrameFor(ProductShotPlatform.ios).frameSize,
    //   targetPlatform: TargetPlatform.iOS,
    //   (tester) async {
    //     await tester.pumpWidget(buildFramelessSubject(ProductShotPlatform.ios));
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("group_chat", ProductShotPlatform.ios),
    //       ),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Group Chat (${_deviceFrameFor(ProductShotPlatform.android).name})",
    //   hostPlatform: "linux",
    //   physicalSize: _deviceFrameFor(ProductShotPlatform.android).frameSize,
    //   targetPlatform: TargetPlatform.android,
    //   (tester) async {
    //     await tester.pumpWidget(
    //       buildFramelessSubject(ProductShotPlatform.android),
    //     );
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("group_chat", ProductShotPlatform.android),
    //       ),
    //     );
    //   },
    // );
  });

  group("macOS Product Shots", () {
    late MockNavigationCubit navigationCubit;
    late MockUserCubit userCubit;
    late MockUsersCubit usersCubit;
    late MockChatDetailsCubit chatDetailsCubit;
    late MockMessageListCubit messageListCubit;
    late MockUserSettingsCubit userSettingsCubit;
    late MockAttachmentsRepository attachmentsRepository;

    setUp(() async {
      navigationCubit = MockNavigationCubit();
      userCubit = MockUserCubit();
      usersCubit = MockUsersCubit();
      chatDetailsCubit = MockChatDetailsCubit();
      messageListCubit = MockMessageListCubit();
      userSettingsCubit = MockUserSettingsCubit();
      attachmentsRepository = MockAttachmentsRepository();

      when(() => userCubit.state).thenReturn(MockUiUser(id: ownIdx));
      when(() => usersCubit.state).thenReturn(
        MockUsersState(profiles: userProfiles, defaultUserId: ownId),
      );
      when(
        () => userSettingsCubit.state,
      ).thenReturn(const UserSettings(experimentalFeatures: false));
      when(
        () => chatDetailsCubit.markAsRead(
          untilMessageId: any(named: "untilMessageId"),
          untilTimestamp: any(named: "untilTimestamp"),
        ),
      ).thenAnswer((_) => Future.value());
      when(
        () => chatDetailsCubit.storeDraft(
          draftMessage: any(named: "draftMessage"),
          isCommitted: any(named: "isCommitted"),
        ),
      ).thenAnswer((_) async => Future.value());
      when(
        () => attachmentsRepository.statusStream(
          attachmentId: any(named: "attachmentId"),
        ),
      ).thenAnswer((_) => Stream.value(const UiAttachmentStatus.completed()));
    });

    Widget buildSubject({
      required Color backgroundColor,
      required Color titleColor,
      required Color subtitleColor,
      required Color frameColor,
      required String title,
      required String subtitle,
    }) => MultiRepositoryProvider(
      providers: [
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
        ),
        RepositoryProvider<chats_repository.ChatsRepository>.value(
          value: FakeChatsRepository(chats),
        ),
      ],
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<MessageListCubit>.value(value: messageListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              final shot = ProductShot(
                // size: macosProductShotSize,
                backgroundColor: backgroundColor,
                titleColor: titleColor,
                subtitleColor: subtitleColor,
                title: title,
                subtitle: subtitle,
                frameColor: frameColor,
                device: TargetPlatform.macOS.device,
                child: const HomeScreenDesktopLayout(
                  chatList: ChatListView(),
                  chat: ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                ),
              );

              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testLightTheme,
                themeMode: ThemeMode.light,
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: Material(
                  child: MediaQuery(
                    data: MediaQuery.of(
                      context,
                    ).copyWith(platformBrightness: .light),
                    child: shot,
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );

    Widget buildFramelessSubject() => MultiRepositoryProvider(
      providers: [
        RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
        ),
        RepositoryProvider<chats_repository.ChatsRepository>.value(
          value: FakeChatsRepository(chats),
        ),
      ],
      child: MultiBlocProvider(
        providers: [
          BlocProvider<NavigationCubit>.value(value: navigationCubit),
          BlocProvider<UserCubit>.value(value: userCubit),
          BlocProvider<UsersCubit>.value(value: usersCubit),
          BlocProvider<ChatDetailsCubit>.value(value: chatDetailsCubit),
          BlocProvider<MessageListCubit>.value(value: messageListCubit),
          BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
        ],
        child: SDTFScope(
          child: Builder(
            builder: (context) {
              final shot = DeviceFrame(
                device: TargetPlatform.macOS.device.deviceInfo,
                screen: const HomeScreenDesktopLayout(
                  chatList: ChatListView(),
                  chat: ChatScreenView(
                    createMessageCubit: createMockMessageCubit,
                  ),
                ),
              );

              return MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testLightTheme,
                themeMode: ThemeMode.light,
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: Material(
                  color: Colors.transparent,
                  child: MediaQuery(
                    data: MediaQuery.of(
                      context,
                    ).copyWith(platformBrightness: .light),
                    child: Center(child: shot),
                  ),
                ),
              );
            },
          ),
        ),
      ),
    );

    // testProductShot(
    //   "Private Chat (macOS)",
    //   hostPlatform: "macos",
    //   physicalSize: macosPhysicalSize,
    //   targetPlatform: TargetPlatform.macOS,
    //   (tester) async {
    //     final chat = privateChat;
    //     when(() => navigationCubit.state).thenReturn(
    //       NavigationState.home(
    //         home: HomeNavigationState(chatOpen: true, chatId: chat.id),
    //       ),
    //     );
    //     when(
    //       () => chatDetailsCubit.state,
    //     ).thenReturn(ChatDetailsState(chat: chat, members: privateChatMembers));
    //     messageListCubit.setState(privateChatMessages);
    //     _stubAttachments(attachmentsRepository, privateChatAttachmentImages);

    //     // The desktop layout always shows the chat list, so this shot doubles
    //     // as the hero image and carries the lead store copy.
    //     await tester.pumpWidget(
    //       buildSubject(
    //         backgroundColor: Primitive.neutral(NeutralShade.s100),
    //         titleColor: Primitive.neutral(NeutralShade.s800),
    //         subtitleColor: Primitive.neutral(NeutralShade.s600),
    //         frameColor: Primitive.neutral(NeutralShade.s300),
    //         title: 'Secure messaging for everyone.',
    //         subtitle: 'Everything in Air is end-to-end encrypted.',
    //       ),
    //     );
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/macos/screenshots/en-US
    //       matchesGoldenFile(_golden("private_chat.macos.png")),
    //     );

    //     tester.view.physicalSize =
    //         TargetPlatform.macOS.device.deviceInfo.frameSize;
    //     await tester.pumpWidget(buildFramelessSubject());
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("private_chat", ProductShotPlatform.macos),
    //       ),
    //     );
    //   },
    // );

    // testProductShot(
    //   "Group Chat (macOS)",
    //   hostPlatform: "macos",
    //   physicalSize: macosPhysicalSize,
    //   targetPlatform: TargetPlatform.macOS,
    //   (tester) async {
    //     final chat = groupChat;
    //     when(() => navigationCubit.state).thenReturn(
    //       NavigationState.home(
    //         home: HomeNavigationState(chatOpen: true, chatId: chat.id),
    //       ),
    //     );
    //     when(
    //       () => chatDetailsCubit.state,
    //     ).thenReturn(ChatDetailsState(chat: chat, members: groupChatMembers));
    //     messageListCubit.setState(groupChatMessages);
    //     _stubAttachments(attachmentsRepository, groupChatAttachmentImages);

    //     await tester.pumpWidget(
    //       buildSubject(
    //         backgroundColor: Primitive.chromatic(Hue.blue, Shade.s50),
    //         titleColor: Primitive.chromatic(Hue.blue, Shade.s800),
    //         subtitleColor: Primitive.chromatic(Hue.blue, Shade.s600),
    //         frameColor: Primitive.chromatic(Hue.blue, Shade.s300),
    //         title: 'Create group chats.',
    //         subtitle: 'Message with multiple people.',
    //       ),
    //     );
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(ProductShot),
    //       // Do not change the file name, as it is referenced in stores/macos/screenshots/en-US
    //       matchesGoldenFile(_golden("group_chat.macos.png")),
    //     );

    //     tester.view.physicalSize = _deviceFrameFor(
    //       ProductShotPlatform.macos,
    //     ).frameSize;
    //     await tester.pumpWidget(buildFramelessSubject());
    //     await _precacheImages(tester);
    //     await tester.pumpAndSettle();

    //     await expectLater(
    //       find.byType(DeviceFrame),
    //       matchesGoldenFile(
    //         _deviceGolden("group_chat", ProductShotPlatform.macos),
    //       ),
    //     );
    //   },
    // );
  });
}

/// Stubs each attachment id to its own image, since the mocked repository
/// otherwise can't tell which attachment is being requested.
void _stubAttachments(
  MockAttachmentsRepository attachmentsRepository,
  Map<AttachmentId, ImageData> images,
) {
  for (final entry in images.entries) {
    when(
      () => attachmentsRepository.loadImageAttachment(
        attachmentId: entry.key,
        retryDownloadIfFailed: false,
        chunkEventCallback: any(named: "chunkEventCallback"),
      ),
    ).thenAnswer(
      (_) => Future.value(
        LoadedImageAttachment(bytes: entry.value.data, isAnimated: false),
      ),
    );
  }
}

/// [hostPlatform] is the OS that records the shot, not the device it depicts:
/// the iOS shots are recorded on macOS because that is where the San Francisco
/// system font is available. [targetPlatform] is the depicted platform, which
/// drives the typescale and the device type independently of the host.
void testProductShot(
  String description,
  WidgetTesterCallback callback, {
  required String hostPlatform,
  // required Size physicalSize,
  required TargetPlatform targetPlatform,
}) async {
  testWidgets(
    description,
    (tester) async {
      debugDisableShadows = false;

      tester.view.physicalSize = targetPlatform.device.screenSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      try {
        await callback(tester);
      } finally {
        debugDisableShadows = true;
      }
    },
    skip: Platform.operatingSystem != hostPlatform,
    variant: TargetPlatformVariant.only(targetPlatform),
  );
}

/// Preload all images in the widget tree.
///
/// This is necessary in tests, otherwise the images will not be rendered.
///
/// Will be called inside `tester.runAsync`. Otherwise, `precacheImage` will
/// never complete due to fake-async.
Future<void> _precacheImages(WidgetTester tester) async {
  // [AttachmentImage] inserts its [Image] child only after an async
  // classification step resolves; settle once so the tree contains every
  // [Image] we need to precache.
  await tester.pumpAndSettle();
  await tester.runAsync(() async {
    final elements = tester.elementList(find.byType(DecoratedBox));
    for (Element element in elements) {
      final widget = element.widget as DecoratedBox;
      final image = switch (widget.decoration) {
        BoxDecoration(:final image) => image,
        ShapeDecoration(:final image) => image,
        _ => null,
      };
      if (image != null) {
        await precacheImage(image.image, element);
      }
    }

    final attachmentElements = tester.elementList(find.byType(Image));
    for (Element element in attachmentElements) {
      final image = element.widget as Image;
      await precacheImage(image.image, element);
    }
  });
}
