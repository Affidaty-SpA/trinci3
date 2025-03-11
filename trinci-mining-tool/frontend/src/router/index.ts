import { createRouter, createWebHistory } from '@ionic/vue-router';
import HomeView from '../views/Dashboard/HomeView.vue'
import { useAuthIn } from '@/utils/useAuthin';
import type { NavigationGuardNext, RouteLocationNormalized } from 'vue-router';
import { useUserStore } from '@/stores/user.store';
import Step1SetupView from '@/views/Wizard/Step1SetupView.vue';
import Step2SelectionView from '@/views/Wizard/Step2SelectionView.vue';
import NodeView from '@/views/Nodes/NodeView.vue';

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'init',
      redirect: '/login'

    },
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue')
    },
    {
      path: '/check',
      name: 'checkStatus',
      component: () => import('@/views/CheckStatus.vue')
    },
    {
      path: '/wizard/step1',
      name: 'step1setup',
      component: Step1SetupView
    },
    {
      path: '/wizard/step2',
      name: 'step2selection',
      component: Step2SelectionView
    },
    {
      path: '/dashboard/home',
      name: 'home',
      component: HomeView
    },
    {
      path: '/dashboard/node/:accountId',
      name: 'node',
      component: NodeView
    }
  ]
})

export type TrinciTokenAuth = {
  token: string,
  expire: Date
}

router.beforeEach((to: RouteLocationNormalized, from: RouteLocationNormalized, next: NavigationGuardNext) => {
  // redirect to login page if not logged in and trying to access a restricted page
  const publicPages = ['/login'];
    const tk = localStorage.getItem("trinci_token");
    if(tk && tk != "") {
      const tempToken: any = JSON.parse(tk);
      const token: TrinciTokenAuth = {token: tempToken.token, expire: new Date(tempToken.expire)}
      const now = new Date();
      if((now.getTime() - token.expire.getTime()) > 0 ) { // token expired!
        useAuthIn().logout();
        return next({ name: 'login' })
      }
      if(to.path == "/login") { // prevento back on login from authenticated page or refresh on login when user is already althenticated
        return next({ name: 'home' })
      }
       const { beforeEnterPage } = useUserStore()
      // TODO: altrimenti interpreto il token e faccio la logica dell'after_login e risolvo to.path
      return beforeEnterPage(token, next)
    } else if(publicPages.includes(to.path)) {
      next()
    } else {
      return next({ name: 'login' })
    }
  });

export default router
