// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'user_session_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UserSessionState {

 User? get user;/// True once we know there is no session (never logged in, or logged out).
/// False at startup while the default user is still loading.
 bool get loggedOut;
/// Create a copy of UserSessionState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UserSessionStateCopyWith<UserSessionState> get copyWith => _$UserSessionStateCopyWithImpl<UserSessionState>(this as UserSessionState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UserSessionState&&(identical(other.user, user) || other.user == user)&&(identical(other.loggedOut, loggedOut) || other.loggedOut == loggedOut));
}


@override
int get hashCode => Object.hash(runtimeType,user,loggedOut);

@override
String toString() {
  return 'UserSessionState(user: $user, loggedOut: $loggedOut)';
}


}

/// @nodoc
abstract mixin class $UserSessionStateCopyWith<$Res>  {
  factory $UserSessionStateCopyWith(UserSessionState value, $Res Function(UserSessionState) _then) = _$UserSessionStateCopyWithImpl;
@useResult
$Res call({
 User? user, bool loggedOut
});




}
/// @nodoc
class _$UserSessionStateCopyWithImpl<$Res>
    implements $UserSessionStateCopyWith<$Res> {
  _$UserSessionStateCopyWithImpl(this._self, this._then);

  final UserSessionState _self;
  final $Res Function(UserSessionState) _then;

/// Create a copy of UserSessionState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? user = freezed,Object? loggedOut = null,}) {
  return _then(_self.copyWith(
user: freezed == user ? _self.user : user // ignore: cast_nullable_to_non_nullable
as User?,loggedOut: null == loggedOut ? _self.loggedOut : loggedOut // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

}



/// @nodoc


class _UserSessionState extends UserSessionState {
  const _UserSessionState({this.user, this.loggedOut = false}): super._();
  

@override final  User? user;
/// True once we know there is no session (never logged in, or logged out).
/// False at startup while the default user is still loading.
@override@JsonKey() final  bool loggedOut;

/// Create a copy of UserSessionState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UserSessionStateCopyWith<_UserSessionState> get copyWith => __$UserSessionStateCopyWithImpl<_UserSessionState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _UserSessionState&&(identical(other.user, user) || other.user == user)&&(identical(other.loggedOut, loggedOut) || other.loggedOut == loggedOut));
}


@override
int get hashCode => Object.hash(runtimeType,user,loggedOut);

@override
String toString() {
  return 'UserSessionState(user: $user, loggedOut: $loggedOut)';
}


}

/// @nodoc
abstract mixin class _$UserSessionStateCopyWith<$Res> implements $UserSessionStateCopyWith<$Res> {
  factory _$UserSessionStateCopyWith(_UserSessionState value, $Res Function(_UserSessionState) _then) = __$UserSessionStateCopyWithImpl;
@override @useResult
$Res call({
 User? user, bool loggedOut
});




}
/// @nodoc
class __$UserSessionStateCopyWithImpl<$Res>
    implements _$UserSessionStateCopyWith<$Res> {
  __$UserSessionStateCopyWithImpl(this._self, this._then);

  final _UserSessionState _self;
  final $Res Function(_UserSessionState) _then;

/// Create a copy of UserSessionState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? user = freezed,Object? loggedOut = null,}) {
  return _then(_UserSessionState(
user: freezed == user ? _self.user : user // ignore: cast_nullable_to_non_nullable
as User?,loggedOut: null == loggedOut ? _self.loggedOut : loggedOut // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

// dart format on
