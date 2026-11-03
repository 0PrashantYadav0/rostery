import { mount } from 'svelte';
import type { SharePayload } from '../shared/types';
import '../app/styles.css';
import Share from './Share.svelte';
import './styles.css';

const data = JSON.parse(document.getElementById('rostery-data')!.textContent!) as SharePayload;
mount(Share, { target: document.getElementById('share')!, props: { data } });
