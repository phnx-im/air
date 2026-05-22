// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'user_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$InviteUsersError {

 String get reason;
/// Create a copy of InviteUsersError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InviteUsersErrorCopyWith<InviteUsersError> get copyWith => _$InviteUsersErrorCopyWithImpl<InviteUsersError>(this as InviteUsersError, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InviteUsersError&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'InviteUsersError(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $InviteUsersErrorCopyWith<$Res>  {
  factory $InviteUsersErrorCopyWith(InviteUsersError value, $Res Function(InviteUsersError) _then) = _$InviteUsersErrorCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$InviteUsersErrorCopyWithImpl<$Res>
    implements $InviteUsersErrorCopyWith<$Res> {
  _$InviteUsersErrorCopyWithImpl(this._self, this._then);

  final InviteUsersError _self;
  final $Res Function(InviteUsersError) _then;

/// Create a copy of InviteUsersError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? reason = null,}) {
  return _then(_self.copyWith(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}

}



/// @nodoc


class InviteUsersError_IncompatibleClient extends InviteUsersError {
  const InviteUsersError_IncompatibleClient({required this.reason}): super._();
  

@override final  String reason;

/// Create a copy of InviteUsersError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InviteUsersError_IncompatibleClientCopyWith<InviteUsersError_IncompatibleClient> get copyWith => _$InviteUsersError_IncompatibleClientCopyWithImpl<InviteUsersError_IncompatibleClient>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InviteUsersError_IncompatibleClient&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'InviteUsersError.incompatibleClient(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $InviteUsersError_IncompatibleClientCopyWith<$Res> implements $InviteUsersErrorCopyWith<$Res> {
  factory $InviteUsersError_IncompatibleClientCopyWith(InviteUsersError_IncompatibleClient value, $Res Function(InviteUsersError_IncompatibleClient) _then) = _$InviteUsersError_IncompatibleClientCopyWithImpl;
@override @useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$InviteUsersError_IncompatibleClientCopyWithImpl<$Res>
    implements $InviteUsersError_IncompatibleClientCopyWith<$Res> {
  _$InviteUsersError_IncompatibleClientCopyWithImpl(this._self, this._then);

  final InviteUsersError_IncompatibleClient _self;
  final $Res Function(InviteUsersError_IncompatibleClient) _then;

/// Create a copy of InviteUsersError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(InviteUsersError_IncompatibleClient(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
