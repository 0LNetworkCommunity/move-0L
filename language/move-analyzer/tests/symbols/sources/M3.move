module Symbols::M3 {

    struct ParamStruct<T> {
        some_field: T,
    }

    fun type_param_arg<T>(param: T): T {
        param
    }

    fun struct_type_param_arg<T>(param: ParamStruct<T>): ParamStruct<T> {
        param
    }

    fun pack_type_param<T>(param: T): ParamStruct<T> {
        ParamStruct<T> { some_field: param }
    }

    fun struct_parameterized_arg(param: ParamStruct<u64>): ParamStruct<u64> {
        param
    }

    struct AnotherParamStruct<T> {
        some_field: ParamStruct<T>,
    }

}
