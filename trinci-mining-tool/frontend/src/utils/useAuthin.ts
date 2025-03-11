import { http } from "@/interceptor/http.interceptor";
import { useToast } from "vue-toastification";

export interface AuthinUser {
  publicKey: string;
  encryptedPrivateKey: string;
  userInfo: UserInfo;
}

export interface UserData extends AuthinUser {
  roles?: string[]
}

export interface UserInfo {
  name: string;
  surname: string;
  phone: string;
  email: string;
}

let logoutOnReady: {fire: boolean; callback?: () => void} = {
  fire: false,
  callback: undefined
}

export const useAuthIn = () => {
  const toast = useToast();

  const logout = (callback?: () => void) => {
    // document.cookie = `tk=;expires=Thu, 01 Jan 1970 00:00:00 UTC;`
/*     if (!window.AuthIn4rya){
      logoutOnReady = { fire: true, callback }
      return
    }
    window.AuthIn4rya.logout().then(() => {
      logoutOnReady = {fire: false}
      return callback && callback()
    }) */
    localStorage.removeItem('user_account')
    localStorage.removeItem('userInformations')
    localStorage.removeItem('account')
    localStorage.removeItem('user_information')
    localStorage.removeItem('trinci_token')
  };

  const login = () => {
    return new Promise<{
        token: string;
        tokenExpire: Date,
        userInfo: { logged: boolean; user: UserData };
    }>((resolve, reject) => {
        let preAuth = "";
        let signedData = "";
        let userInfo: { logged: boolean; user: UserData }
        const expirationTime = (window as any).AuthIn4rya.userSessionDuration();
        http<{ preAuth: string }>({
          url: "/api/v1/authIn/preAuth",
          method: "post",
          httpCallOptions: {
            headers: { "Content-Type": "application/json" },
            baseURL: import.meta.env.VITE_API_PROXI
          },
          data: { expirationTime },
          authorization: false,
        }).then((res) => {
            preAuth = res.preAuth;
            return (window as any).AuthIn4rya.login(res.preAuth);
        }).then((signData) => {
            signedData = signData;
            return (window as any).AuthIn4rya.getUserInformations(true);
        }).then((user) => {
            userInfo = user
            return http<{ authIn: string }>({
              url: "/api/v1/authIn/sign",
              method: "post",
              httpCallOptions: {
                headers: { "Content-Type": "application/json" },
                baseURL: import.meta.env.VITE_API_PROXI
              },
              data: {
                preAuth,
                signature: signedData,
                pubKey: user.user.publicKey,
              },
              authorization: false,
            });
        }).then((response) => {
            const date = new Date();
            date.setSeconds(date.getSeconds() + expirationTime);
            // document.cookie = `tk=${response.authIn};expires=${date}`
            const loginEvent = new CustomEvent('affidaty:auth-in:success')
            window.dispatchEvent(loginEvent)
            resolve({
              token: response.authIn,
              tokenExpire: date,
              userInfo
            })
        }).catch((err) => {
            if (userInfo){
              logout();
            }
            console.error(err);
            reject(err)
        });
    })
  };

  return {
    // initAuthInListeners,
    logout,
    login,
  };
};

export const getCookie = (cname: string) => {
  let name = cname + "=";
  let decodedCookie = decodeURIComponent(document.cookie);
  let ca = decodedCookie.split(';');
  for(let i = 0; i <ca.length; i++) {
    let c = ca[i];
    while (c.charAt(0) == ' ') {
      c = c.substring(1);
    }
    if (c.indexOf(name) == 0) {
      return c.substring(name.length, c.length);
    }
  }
  return "";
}
