// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:typed_data';

import 'package:air/core/core.dart';
import 'package:air/navigation/navigation.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Wrapper of the [UserCubitBase] that implements a [StateStreamableSource]
///
// See <https://github.com/phnx-im/air/issues/248>
class UserCubit implements StateStreamableSource<UiUser> {
  UserCubit({
    required CoreClient coreClient,
    required NavigationCubit navigationCubit,
    required Stream<AppState> appStateStream,
  }) : _impl = UserCubitBase(
         user: coreClient.user,
         navigation: navigationCubit.base,
       ) {
    _appStateSubscription = appStateStream.listen((appState) {
      _appState = appState;
      _impl.setAppState(appState: appState);
    });
  }

  final UserCubitBase _impl;
  late final StreamSubscription<AppState> _appStateSubscription;
  AppState _appState = AppState.foreground;

  UserCubitBase get impl => _impl;
  AppState get appState => _appState;

  @override
  FutureOr<void> close() {
    _appStateSubscription.cancel();
    _impl.close();
  }

  @override
  bool get isClosed => _impl.isClosed;

  @override
  UiUser get state => _impl.state;

  @override
  Stream<UiUser> get stream => _impl.stream();

  // Cubit methods

  Future<void> setProfile({String? displayName, Uint8List? profilePicture}) =>
      _impl.setProfile(
        displayName: displayName,
        profilePicture: profilePicture,
      );

  Future<void> addUserToChat(ChatId chatId, UiUserId userId) =>
      _impl.addUserToChat(chatId, userId);

  Future<void> removeUserFromChat(ChatId chatId, UiUserId userId) =>
      _impl.removeUserFromChat(chatId, userId);

  Future<void> leaveChat(ChatId chatId) => _impl.leaveChat(chatId);

  Future<void> deleteChat(ChatId chatId) => _impl.deleteChat(chatId);

  Future<List<UiContact>> get contacts => _impl.contacts;

  Future<bool> addUserHandle(UiUserHandle userHandle) =>
      _impl.addUserHandle(userHandle: userHandle);

  Future<void> removeUserHandle(UiUserHandle userHandle) =>
      _impl.removeUserHandle(userHandle: userHandle);

  Future<List<UiContact>> addableContacts(ChatId chatId) =>
      _impl.addableContacts(chatId: chatId);

  Future<void> blockContact(UiUserId userId) =>
      _impl.blockContact(userId: userId);

  Future<void> unblockContact(UiUserId userId) =>
      _impl.unblockContact(userId: userId);

  Future<void> reportSpam(UiUserId spammerId) =>
      _impl.reportSpam(spammerId: spammerId);

  Future<void> deleteAccount({required String confirmationText}) async =>
      _impl.deleteAccount(
        dbPath: await dbPath(),
        confirmationText: confirmationText,
      );

  Future<UiContact?> contact({required UiUserId userId}) =>
      _impl.contact(userId: userId);

  Future<ChatId> addContactFromGroup({
    required ChatId chatId,
    required UiUserId userId,
  }) => _impl.addContactFromGroup(chatId: chatId, userId: userId);

  Future<intArray12> safetyCodes(UiUserId userId) =>
      _impl.safetyCodes(otherUserId: userId);
}
