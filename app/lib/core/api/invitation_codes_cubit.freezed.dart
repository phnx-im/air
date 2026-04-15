// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'invitation_codes_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$InvitationCode {

 String get code; bool get copied;
/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InvitationCodeCopyWith<InvitationCode> get copyWith => _$InvitationCodeCopyWithImpl<InvitationCode>(this as InvitationCode, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InvitationCode&&(identical(other.code, code) || other.code == code)&&(identical(other.copied, copied) || other.copied == copied));
}


@override
int get hashCode => Object.hash(runtimeType,code,copied);

@override
String toString() {
  return 'InvitationCode(code: $code, copied: $copied)';
}


}

/// @nodoc
abstract mixin class $InvitationCodeCopyWith<$Res>  {
  factory $InvitationCodeCopyWith(InvitationCode value, $Res Function(InvitationCode) _then) = _$InvitationCodeCopyWithImpl;
@useResult
$Res call({
 String code, bool copied
});




}
/// @nodoc
class _$InvitationCodeCopyWithImpl<$Res>
    implements $InvitationCodeCopyWith<$Res> {
  _$InvitationCodeCopyWithImpl(this._self, this._then);

  final InvitationCode _self;
  final $Res Function(InvitationCode) _then;

/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? code = null,Object? copied = null,}) {
  return _then(_self.copyWith(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,copied: null == copied ? _self.copied : copied // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

}



/// @nodoc


class _InvitationCode implements InvitationCode {
  const _InvitationCode({required this.code, required this.copied});
  

@override final  String code;
@override final  bool copied;

/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$InvitationCodeCopyWith<_InvitationCode> get copyWith => __$InvitationCodeCopyWithImpl<_InvitationCode>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _InvitationCode&&(identical(other.code, code) || other.code == code)&&(identical(other.copied, copied) || other.copied == copied));
}


@override
int get hashCode => Object.hash(runtimeType,code,copied);

@override
String toString() {
  return 'InvitationCode(code: $code, copied: $copied)';
}


}

/// @nodoc
abstract mixin class _$InvitationCodeCopyWith<$Res> implements $InvitationCodeCopyWith<$Res> {
  factory _$InvitationCodeCopyWith(_InvitationCode value, $Res Function(_InvitationCode) _then) = __$InvitationCodeCopyWithImpl;
@override @useResult
$Res call({
 String code, bool copied
});




}
/// @nodoc
class __$InvitationCodeCopyWithImpl<$Res>
    implements _$InvitationCodeCopyWith<$Res> {
  __$InvitationCodeCopyWithImpl(this._self, this._then);

  final _InvitationCode _self;
  final $Res Function(_InvitationCode) _then;

/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? code = null,Object? copied = null,}) {
  return _then(_InvitationCode(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,copied: null == copied ? _self.copied : copied // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc
mixin _$InvitationCodesState {

 List<InvitationCode> get codes;
/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InvitationCodesStateCopyWith<InvitationCodesState> get copyWith => _$InvitationCodesStateCopyWithImpl<InvitationCodesState>(this as InvitationCodesState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InvitationCodesState&&const DeepCollectionEquality().equals(other.codes, codes));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(codes));

@override
String toString() {
  return 'InvitationCodesState(codes: $codes)';
}


}

/// @nodoc
abstract mixin class $InvitationCodesStateCopyWith<$Res>  {
  factory $InvitationCodesStateCopyWith(InvitationCodesState value, $Res Function(InvitationCodesState) _then) = _$InvitationCodesStateCopyWithImpl;
@useResult
$Res call({
 List<InvitationCode> codes
});




}
/// @nodoc
class _$InvitationCodesStateCopyWithImpl<$Res>
    implements $InvitationCodesStateCopyWith<$Res> {
  _$InvitationCodesStateCopyWithImpl(this._self, this._then);

  final InvitationCodesState _self;
  final $Res Function(InvitationCodesState) _then;

/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? codes = null,}) {
  return _then(_self.copyWith(
codes: null == codes ? _self.codes : codes // ignore: cast_nullable_to_non_nullable
as List<InvitationCode>,
  ));
}

}



/// @nodoc


class _InvitationCodesState extends InvitationCodesState {
  const _InvitationCodesState({required final  List<InvitationCode> codes}): _codes = codes,super._();
  

 final  List<InvitationCode> _codes;
@override List<InvitationCode> get codes {
  if (_codes is EqualUnmodifiableListView) return _codes;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_codes);
}


/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$InvitationCodesStateCopyWith<_InvitationCodesState> get copyWith => __$InvitationCodesStateCopyWithImpl<_InvitationCodesState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _InvitationCodesState&&const DeepCollectionEquality().equals(other._codes, _codes));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_codes));

@override
String toString() {
  return 'InvitationCodesState(codes: $codes)';
}


}

/// @nodoc
abstract mixin class _$InvitationCodesStateCopyWith<$Res> implements $InvitationCodesStateCopyWith<$Res> {
  factory _$InvitationCodesStateCopyWith(_InvitationCodesState value, $Res Function(_InvitationCodesState) _then) = __$InvitationCodesStateCopyWithImpl;
@override @useResult
$Res call({
 List<InvitationCode> codes
});




}
/// @nodoc
class __$InvitationCodesStateCopyWithImpl<$Res>
    implements _$InvitationCodesStateCopyWith<$Res> {
  __$InvitationCodesStateCopyWithImpl(this._self, this._then);

  final _InvitationCodesState _self;
  final $Res Function(_InvitationCodesState) _then;

/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? codes = null,}) {
  return _then(_InvitationCodesState(
codes: null == codes ? _self._codes : codes // ignore: cast_nullable_to_non_nullable
as List<InvitationCode>,
  ));
}


}

// dart format on
