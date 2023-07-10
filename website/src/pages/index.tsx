import React, { useEffect, useState } from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';

import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();

  const carouselItems = ['potent', 'scient', 'present']; // Array of carousel items

  const [currentItemIndex, setCurrentItemIndex] = useState(0); // State to track the current item index

  useEffect(() => {
    // Function to handle automatic carousel item change
    const changeCarouselItem = () => {
      setCurrentItemIndex((prevIndex) => (prevIndex + 1) % carouselItems.length);
    };

    // Set an interval to change the carousel item every 3 seconds
    const interval = setInterval(changeCarouselItem, 3000);

    // Clean up the interval when the component unmounts
    return () => {
      clearInterval(interval);
    };
  }, []);

  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <h1 className="hero__title">{siteConfig.title}</h1>
        <p className="hero__subtitle">The <span className={styles.omnitext}>omni{carouselItems[currentItemIndex]}</span> dev tool</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/tutorials/get-started">
            Get Started - 5min ⏱️
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home(): JSX.Element {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={`${siteConfig.title}, ${siteConfig.tagline}`}
      description={`${siteConfig.title} is the omnipotent, omniscient, and omnipresent dev tool that enhances your command-line. Simplify command management, discover commands and repositories effortlessly, and execute commands from anywhere in your system. Let omni enhances your productivity in no time!`}>
      <HomepageHeader />
      <main>
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
