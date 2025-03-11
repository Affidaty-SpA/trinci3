import _axios, { AxiosError, type AxiosRequestConfig } from "axios";

export interface HttpOptions {
    method: "get" | "post" | "delete" | "patch" | "put";
    url: string;
    loader?: boolean;
    data?: any;
    authorization?: boolean | string;
    httpCallOptions?: AxiosRequestConfig;
}

const axios = _axios
axios.defaults.headers.common = {
    'Accept': 'application/json',
    'Content-Type': 'application/json',
}

export const handleError = (error: AxiosError<any>): AxiosError => {
    //TODO: implement internationalization
    //const { t } = useI18nAffidaty()
    if (error.response) {
        // The request was made and the server responded with a status code
        // that falls out of the range of 2xx
        console.log(error.response.data)
        // Log.log(error.response.status)
        // Log.log(error.response.headers)
        const response = error.response?.data
        if(response){
            // trigger alert message error
            switch (error.response.status){
                case 400:
                    break
                case 401:
                    //TODO: implement translations
                    break
                case 403:
                    break
                case 422:
                    break
                case 500:
                    break
                case 504:
                    //store.commit('ui/setAlertMessage', {type: 'error', title: 'Service unavailable'}, {root: true})
                    break
            }
        }

    } else if (error.request) {
        // The request was made but no response was received
        // `error.request` is an instance of XMLHttpRequest in the browser and an instance of
        // http.ClientRequest in node.js
        console.log(error.request)
    } else {
        // Something happened in setting up the request that triggered an Error
        console.log('Error', error.message)
    }

    return error
}

export const http = <T>(handler: HttpOptions): Promise<T> => {
    handler = {
        ...handler,
        httpCallOptions: {
            ...handler.httpCallOptions,
            baseURL: handler.httpCallOptions?.baseURL? handler.httpCallOptions.baseURL : "" // import.meta.env.VITE_REST_ENDPOINT
        }
    }
    const {
        httpCallOptions,
        method,
        url,
        data,
    } = handler

    // const bearerAuth = Cookies.get('tk_pay')? true : false
    const cacheHeader: object = method == 'get' ? {'Cache-Control': 'no-cache' } : {}
    // const authHeader: object = bearerAuth ? {'Authorization': `Bearer ${Cookies.get('tk_pay')}`} : {}

    return axios.request<T>({
        url,
        method,
        data,
        ...httpCallOptions,
        headers: {
            ...httpCallOptions?.headers,
            ...cacheHeader,
            // ...authHeader
        }
    }).then(result => {
        return result.data
    }).catch(error => {
        throw handleError(error)
    })
}
